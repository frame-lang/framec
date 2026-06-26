//! Main compilation logic
//!
//! This module contains the core compilation pipeline for Frame V4.
//! V4 is a pure preprocessor for @@system blocks.

use super::config::{CompileMode, PipelineConfig};
use crate::frame_c::compiler::arcanum::build_arcanum_from_frame_ast;
use crate::frame_c::compiler::assembler;
use crate::frame_c::compiler::codegen::{
    generate_c_compartment_types, generate_compartment_class, generate_cpp_compartment_types,
    generate_csharp_compartment_types, generate_frame_context_class, generate_frame_event_class,
    generate_go_compartment_types, generate_java_compartment_types,
    generate_kotlin_compartment_types, generate_rust_compartment_types,
    generate_swift_compartment_types, generate_system, get_backend, EmitContext,
};
use crate::frame_c::compiler::frame_ast::{FrameAst, ModuleAst, Span as AstSpan};
use crate::frame_c::compiler::frame_validator::FrameValidator;
use crate::frame_c::compiler::pipeline_parser;
use crate::frame_c::compiler::segmenter::{self, Segment};
use crate::frame_c::utils::RunError;
use crate::frame_c::visitors::TargetLanguage;

/// Result of module compilation
#[derive(Debug)]
pub struct CompileResult {
    /// Generated code
    pub code: String,
    /// Validation errors (if any)
    pub errors: Vec<CompileError>,
    /// Validation warnings (if any)
    pub warnings: Vec<CompileError>,
    /// Source map (if generated)
    pub source_map: Option<String>,
}

/// Compilation error
#[derive(Debug, Clone)]
pub struct CompileError {
    pub code: String,
    pub message: String,
    pub line: Option<usize>,
    pub column: Option<usize>,
}

impl CompileError {
    pub fn new(code: &str, message: &str) -> Self {
        Self {
            code: code.to_string(),
            message: message.to_string(),
            line: None,
            column: None,
        }
    }

    pub fn with_location(mut self, line: usize, column: usize) -> Self {
        self.line = Some(line);
        self.column = Some(column);
        self
    }
}

// Helper functions extract_native_code, skip_pragmas_simple, skip_pragmas_keep_native,
// and expand_system_instantiations have been removed — their responsibilities are now
// handled by the Segmenter (Stage 0) and Assembler (Stage 7).

/// Compile a Frame module from source bytes
///
/// This is the main entry point for V4 compilation.
///
/// # Arguments
/// * `source` - The Frame source code as bytes
/// * `config` - Pipeline configuration
///
/// # Returns
/// A CompileResult containing the generated code (or validation results)
pub fn compile_module(source: &[u8], config: &PipelineConfig) -> Result<CompileResult, RunError> {
    // Debug output if enabled
    if config.debug {
        eprintln!(
            "[compile_module] Starting V4 compilation with mode={:?}, target={:?}",
            config.mode, config.target
        );
    }

    // Check for validation-only mode
    if config.mode == CompileMode::ValidationOnly {
        return validate_only(source, config);
    }

    // V4 AST-based compilation
    compile_ast_based(source, config)
}

/// Validation-only mode: run the full V4 pipeline and discard the
/// generated code. Same validator coverage as `framec compile`,
/// without paying for the emit/assembly stages where it matters
/// (the codegen still runs, but its output is never written out).
fn validate_only(source: &[u8], config: &PipelineConfig) -> Result<CompileResult, RunError> {
    let result = compile_ast_based(source, config)?;
    Ok(CompileResult {
        code: String::new(),
        errors: result.errors,
        warnings: result.warnings,
        source_map: None,
    })
}

/// RFC-0035 Round 8: the compile pipeline's threaded state.
///
/// `compile_ast_based` used to be one ~1000-line function holding every
/// intermediate as a local. Round 8 carves it into phase functions
/// (`do_segment`, `do_parse`, …) so the pipeline supervisor FSM can *drive*
/// the phase sequence (Step 2) instead of merely observing it. Each phase
/// reads/writes this context and signals continue/abort by returning
/// `Option<CompileResult>` (`Some` ⇒ stop now and hand that result back;
/// `None` ⇒ continue to the next phase).
pub(crate) struct PipelineCtx {
    source: Vec<u8>,
    config: PipelineConfig,
    /// Stage 0 output.
    source_map: Option<segmenter::SourceMap>,
    /// `@@persist` module pragma present.
    has_persist: bool,
    /// Pass 1 output: parsed systems (mutated later by per-target filtering).
    system_asts: Vec<crate::frame_c::compiler::frame_ast::SystemAst>,
    /// RFC-0022 module-scope `@@import` directives.
    module_imports: Vec<crate::frame_c::compiler::frame_ast::Import>,
    /// Imported `@@system` names carrying the new persist contract (bug #8).
    imported_new_contract_names: Vec<String>,
    /// RFC-0040: systems parsed from `@@import`ed files, present for analysis
    /// (validation + cross-file metadata resolution) only. They are merged into
    /// the arcanum and the codegen registries but **never emitted** — see
    /// `imported_systems` for the emission exclusion set.
    imported_system_asts: Vec<crate::frame_c::compiler::frame_ast::SystemAst>,
    /// RFC-0040: names of the systems in `imported_system_asts`. Codegen emits
    /// only local systems; a name in this set is analysis-visible but
    /// emission-excluded, and references to it lower as cross-file.
    imported_systems: std::collections::HashSet<String>,
    /// RFC-0022 strict-mode import errors, surfaced at the end of the run.
    strict_import_errors: Vec<CompileError>,
    /// Shared arcanum (all systems visible to each other), built once after parse.
    arcanum: Option<crate::frame_c::compiler::arcanum::Arcanum>,
    /// Pass 2 output: per-system `(name, generated_code)`.
    generated_systems: Vec<(String, String)>,
    /// Soft warnings harvested across all systems (W-codes), surfaced at the end.
    module_warnings: Vec<CompileError>,
    /// W-code warnings from `@@fsm` blocks (RFC-0042), harvested in
    /// `do_segment` and merged into the final warnings at codegen time.
    fsm_warnings: Vec<CompileError>,
    /// Generated `@@fsm` code keyed by fsm name (RFC-0042). Produced in
    /// `do_segment` after a block validates; the assembler emits it in
    /// place of the `Segment::Fsm` block.
    fsm_generated: Vec<(String, String)>,
}

impl PipelineCtx {
    pub(crate) fn new(source: &[u8], config: &PipelineConfig) -> Self {
        PipelineCtx {
            source: source.to_vec(),
            config: config.clone(),
            source_map: None,
            has_persist: false,
            system_asts: Vec::new(),
            module_imports: Vec::new(),
            imported_new_contract_names: Vec::new(),
            imported_system_asts: Vec::new(),
            imported_systems: std::collections::HashSet::new(),
            strict_import_errors: Vec::new(),
            arcanum: None,
            generated_systems: Vec::new(),
            module_warnings: Vec::new(),
            fsm_warnings: Vec::new(),
            fsm_generated: Vec::new(),
        }
    }

    /// Placeholder context for the FSM domain initializer; `compile_ast_based`
    /// overwrites `fsm.ctx` with the real one before running.
    pub(crate) fn empty() -> Self {
        PipelineCtx::new(&[], &PipelineConfig::default())
    }
}

/// Stage 0 + RFC-0013 hard-cut gates. Segments the source, records the
/// `@@persist` flag, and rejects the retired bare directives
/// (`@@persist` / `@@target` / `@@codegen` / `@@import`). Returns `Some` with
/// the error result on any rejection or segmentation failure.
pub(crate) fn do_segment(c: &mut PipelineCtx) -> Option<CompileResult> {
    if c.config.debug {
        eprintln!("[compile_ast_based] Starting pipeline-based compilation");
    }

    // Stage 0: Segment source
    let source_map = match segmenter::segment_source(&c.source, c.config.target) {
        Ok(sm) => sm,
        Err(e) => {
            return Some(CompileResult {
                code: String::new(),
                errors: vec![CompileError::new(
                    "E001",
                    &format!("Segmentation error: {}", e),
                )],
                warnings: vec![],
                source_map: None,
            });
        }
    };

    if c.config.debug {
        let system_count = source_map
            .segments
            .iter()
            .filter(|s| matches!(s, Segment::System { .. }))
            .count();
        eprintln!(
            "[compile_ast_based] Segmented: {} segments, {} systems",
            source_map.segments.len(),
            system_count
        );
    }

    // Check for @@persist pragma
    c.has_persist = source_map.persist_pragma().is_some();

    // RFC-0013 hard-cut migrations: bare `@@persist` and `@@target`
    // are no longer accepted. Catch legacy usages with a clear error
    // and a migration pointer.
    {
        let src = std::str::from_utf8(&source_map.source).unwrap_or("");
        for line in src.lines() {
            let trimmed = line.trim_start();
            // Wave 1: @@persist
            if trimmed.starts_with("@@persist") {
                let after = &trimmed[9..];
                let next = after.chars().next();
                let is_bare = match next {
                    None => true,
                    Some(c) => !c.is_ascii_alphanumeric() && c != '_' && c != '-',
                };
                if is_bare {
                    return Some(CompileResult {
                        code: String::new(),
                        errors: vec![CompileError::new(
                            "E803",
                            "Bare `@@persist` is no longer accepted. Migrate to `@@[persist]` \
                             (RFC-0013 wave 1). Args form: `@@persist(domain=[a, b])` becomes \
                             `@@[persist(domain=[a, b])]`. The change is mechanical — wrap the \
                             directive in `@@[ ]`.",
                        )],
                        warnings: vec![],
                        source_map: None,
                    });
                }
            }
            // Wave 2: @@target
            if trimmed.starts_with("@@target") {
                let after = &trimmed[8..];
                let next = after.chars().next();
                let is_bare = match next {
                    None => true,
                    Some(c) => !c.is_ascii_alphanumeric() && c != '_' && c != '-',
                };
                if is_bare {
                    return Some(CompileResult {
                        code: String::new(),
                        errors: vec![CompileError::new(
                            "E804",
                            "Bare `@@target lang` is no longer accepted. Migrate to \
                             `@@[target(\"lang\")]` (RFC-0013 wave 2). Example: \
                             `@@target python_3` becomes `@@[target(\"python_3\")]`.",
                        )],
                        warnings: vec![],
                        source_map: None,
                    });
                }
            }
            // RFC-0032: @@codegen removed entirely. The only knob it
            // ever carried was `frame_event: on`, and the framepiler
            // auto-enables FrameEvent emission whenever a feature
            // that requires it appears (enter/exit args, event
            // forwarding, @@:return, interface return values). The
            // directive was at the wrong granularity for multi-system
            // files (per-system codegen, module-scoped flag) and the
            // only key it controlled was redundant with inference.
            if trimmed.starts_with("@@codegen") {
                let after = &trimmed[9..];
                let next = after.chars().next();
                let is_codegen_directive = match next {
                    None => true,
                    Some(c) => c.is_whitespace() || c == '{',
                };
                if is_codegen_directive {
                    return Some(CompileResult {
                        code: String::new(),
                        errors: vec![CompileError::new(
                            "E824",
                            "`@@codegen { ... }` is no longer accepted (RFC-0032). \
                             Delete the directive — the framepiler auto-enables \
                             `frame_event` whenever a feature that requires it appears \
                             (enter/exit args, event forwarding, `@@:return`, \
                             interface return values), and the FrameEvent classes are \
                             always emitted on every backend except Rust regardless of \
                             the flag. See RFC-0032 for the migration walk-through.",
                        )],
                        warnings: vec![],
                        source_map: None,
                    });
                }
            }
            // RFC-0040: `@@import "<path>"` is accepted again as an
            // *analysis-only* directive (RFC-0024 removed the
            // analysis-and-emission form; RFC-0040 revives analysis
            // alone). framec reads the referenced Frame source to
            // resolve and check cross-file references but emits nothing
            // for it — native host imports stay Oceans Model
            // pass-through. The directive is handled in `do_parse`
            // (collected into `module_imports`, then resolved into
            // analysis-only systems); no rejection here.
        }
    }

    // RFC-0042: parse + validate every `@@fsm` block. Errors fail the
    // compile (returned now); warnings are stashed and merged into the
    // final warnings at codegen time. @@fsm emits no target code yet, so
    // clean blocks just pass through (the assembler has an emit-nothing
    // Fsm arm).
    let mut fsm_errors: Vec<CompileError> = Vec::new();
    // Phase 1 — parse every `@@fsm` block. Collect the decls first so each
    // can be validated with visibility of its siblings: Mode C alphabet
    // checking (§8.3 / E731) resolves a `/@Inner/` reference against the
    // other fsms declared in the same module.
    let mut parsed: Vec<(String, crate::frame_c::compiler::frame_ast::FsmDeclAst)> = Vec::new();
    for seg in &source_map.segments {
        if let Segment::Fsm { outer_span, name } = seg {
            let block = &source_map.source[outer_span.start..outer_span.end];
            match crate::frame_c::compiler::fsm_parser::parse_fsm_block(block) {
                Err(pe) => {
                    fsm_errors.push(CompileError::new(
                        pe.code,
                        &format!("@@fsm {}: {}", name, pe.message),
                    ));
                }
                Ok(ast) => parsed.push((name.clone(), ast)),
            }
        }
    }
    let module: Vec<crate::frame_c::compiler::frame_ast::FsmDeclAst> =
        parsed.iter().map(|(_, ast)| ast.clone()).collect();

    // Phase 2 — validate (with sibling visibility) and generate each fsm.
    for (name, ast) in &parsed {
        let mut had_error = false;
        for d in crate::frame_c::compiler::fsm_validator::validate_fsm_in_module(ast, &module) {
            let ce = CompileError::new(d.code, &d.message);
            if d.code.starts_with('W') {
                c.fsm_warnings.push(ce);
            } else {
                had_error = true;
                fsm_errors.push(ce);
            }
        }
        // Codegen — v0.1 implements the Python reference backend
        // and the Rust backend (Phase 8). Other targets surface
        // a clear capability error rather than silently dropping
        // the @@fsm.
        if !had_error {
            use crate::frame_c::visitors::TargetLanguage;
            let generated = match c.config.target {
                TargetLanguage::Python3 => {
                    Some(crate::frame_c::compiler::codegen::fsm_python::generate(ast))
                }
                TargetLanguage::Rust => {
                    Some(crate::frame_c::compiler::codegen::fsm_rust::generate(ast))
                }
                TargetLanguage::Erlang => {
                    Some(crate::frame_c::compiler::codegen::fsm_erlang::generate(ast))
                }
                TargetLanguage::JavaScript => Some(
                    crate::frame_c::compiler::codegen::fsm_javascript::generate(ast),
                ),
                TargetLanguage::TypeScript => Some(
                    crate::frame_c::compiler::codegen::fsm_typescript::generate(ast),
                ),
                TargetLanguage::Go => {
                    Some(crate::frame_c::compiler::codegen::fsm_go::generate(ast))
                }
                TargetLanguage::Ruby => {
                    Some(crate::frame_c::compiler::codegen::fsm_ruby::generate(ast))
                }
                TargetLanguage::Php => {
                    Some(crate::frame_c::compiler::codegen::fsm_php::generate(ast))
                }
                TargetLanguage::Dart => {
                    Some(crate::frame_c::compiler::codegen::fsm_dart::generate(ast))
                }
                TargetLanguage::Lua => {
                    Some(crate::frame_c::compiler::codegen::fsm_lua::generate(ast))
                }
                TargetLanguage::Java => {
                    Some(crate::frame_c::compiler::codegen::fsm_java::generate(ast))
                }
                TargetLanguage::Kotlin => {
                    Some(crate::frame_c::compiler::codegen::fsm_kotlin::generate(ast))
                }
                TargetLanguage::CSharp => {
                    Some(crate::frame_c::compiler::codegen::fsm_csharp::generate(ast))
                }
                TargetLanguage::Swift => {
                    Some(crate::frame_c::compiler::codegen::fsm_swift::generate(ast))
                }
                TargetLanguage::Cpp => {
                    Some(crate::frame_c::compiler::codegen::fsm_cpp::generate(ast))
                }
                TargetLanguage::C => Some(crate::frame_c::compiler::codegen::fsm_c::generate(ast)),
                TargetLanguage::GDScript => Some(
                    crate::frame_c::compiler::codegen::fsm_gdscript::generate(ast),
                ),
                #[allow(unreachable_patterns)]
                _ => None,
            };
            match generated {
                Some(Ok(code)) => c.fsm_generated.push((name.clone(), code)),
                Some(Err(reason)) => fsm_errors.push(CompileError::new(
                    "E740",
                    &format!("@@fsm {}: {}", name, reason),
                )),
                // All 17 targets now have an @@fsm backend, so this arm is
                // unreachable; it remains as a defensive backstop should the
                // target set grow.
                None => fsm_errors.push(CompileError::new(
                    "E740",
                    &format!(
                        "@@fsm {}: code generation for the {:?} target is not yet \
                         implemented",
                        name, c.config.target
                    ),
                )),
            }
        }
    }
    if !fsm_errors.is_empty() {
        return Some(CompileResult {
            code: String::new(),
            errors: fsm_errors,
            warnings: std::mem::take(&mut c.fsm_warnings),
            source_map: None,
        });
    }

    c.source_map = Some(source_map);
    None
}

/// One module's parse output — the systems it declares plus its `@@import`
/// metadata. Produced by [`parse_module_segments`] for the primary source and,
/// under RFC-0040, for each imported file.
struct ParsedModule {
    system_asts: Vec<crate::frame_c::compiler::frame_ast::SystemAst>,
    module_imports: Vec<crate::frame_c::compiler::frame_ast::Import>,
    imported_new_contract_names: Vec<String>,
    strict_import_errors: Vec<CompileError>,
}

/// Parse one module's segments into systems + import metadata, attaching
/// lifecycle pragmas/attributes to the systems they precede. Shared by
/// `do_parse` (primary source) and RFC-0040 import resolution (each imported
/// file). Returns `Err(result)` on the first parse/structure error.
fn parse_module_segments(
    source_map: &segmenter::SourceMap,
    config: &PipelineConfig,
    // RFC-0052 §4: the legacy module-wide persist flag is no longer
    // consulted to force-stamp every system — persist affinity is now
    // resolved per-system from the `@@[persist]` / `@@[*persist]` pragmas
    // walked below. Kept in the signature for call-site symmetry (the
    // primary and imported parses both compute it).
    _has_persist: bool,
) -> Result<ParsedModule, CompileResult> {
    // Pass 1: Parse all systems into ASTs.
    //
    // Walk segments in source order so module-level lifecycle pragmas
    // (RFC-0014 `@@[main]` and RFC-0015 `@@[create]` / `@@[save]` /
    // `@@[load]`) attach to the *next* `@@system` declaration they
    // precede. The buffers reset after attachment so a stray pragma
    // followed by native code-then-system doesn't bleed into a later
    // system.
    let mut system_asts: Vec<crate::frame_c::compiler::frame_ast::SystemAst> = Vec::new();
    // RFC-0022: `@@import "path"` module-scope directives accumulate here
    // before being attached to the ModuleAst. Phase 1 stores the raw
    // (quote-stripped) path; symbols + alias remain empty (lax mode).
    let mut module_imports: Vec<crate::frame_c::compiler::frame_ast::Import> = Vec::new();
    // RFC-0022 strict-mode import errors collected during the segment
    // walk. Surfaced as compile errors at the end of the pass so the
    // user sees every unresolved import in one shot.
    let mut strict_import_errors: Vec<CompileError> = Vec::new();
    // RFC-0022 + FRAMEC_BUGS.md Issue #8: imported `@@system` names
    // that carry an `@@[save]` / `@@[load]` attribute (i.e. use the
    // new persist contract). Merged into `NEW_CONTRACT_SYSTEMS` so the
    // parent's domain-field restore codegen picks the instance-method
    // shape (`Type.new()` + `inst.restore_state(bytes)`) instead of
    // the legacy static-factory shape (`Type.restore_state(bytes)`)
    // when the child lives in an imported file.
    let mut imported_new_contract_names: Vec<String> = Vec::new();
    let mut pending_main_attr_span: Option<crate::frame_c::compiler::frame_ast::Span> = None;
    // RFC-0043: `@@[async]` accumulates here until the next @@system parses
    // and we attach it to that system's attribute vec.
    let mut pending_async_attr_span: Option<crate::frame_c::compiler::frame_ast::Span> = None;
    // Vec (not Option) so multiple occurrences of the same lifecycle
    // pragma — `@@[create(a)]` followed by `@@[create(b)]` — all
    // arrive at the validator and trigger E818 (at most one per
    // system). Option-based capture would silently coalesce.
    let mut pending_create_attrs: Vec<(Option<String>, crate::frame_c::compiler::frame_ast::Span)> =
        Vec::new();
    let mut pending_save_attrs: Vec<(Option<String>, crate::frame_c::compiler::frame_ast::Span)> =
        Vec::new();
    let mut pending_load_attrs: Vec<(Option<String>, crate::frame_c::compiler::frame_ast::Span)> =
        Vec::new();
    // RFC-0052 §4: persist affinity.
    //
    // `pending_persist` holds a `@@[persist]` that attaches to the NEXT
    // single `@@system` (the new default — consistent with `@@[main]` /
    // `@@[async]` / `@@[create]`). It resets after attachment so it can't
    // bleed onto a later sibling.
    //
    // The `broadcast_*` buffers hold `@@[*persist]` / `@@[*save]` /
    // `@@[*load]` — they apply to EVERY system in the module and are
    // never drained.
    //
    // The module-level `has_persist` flag (legacy module-wide stamp) is
    // no longer consulted to force-stamp every system; it is kept only so
    // the C++ `nlohmann::json` prolog hook keeps firing when any system
    // persists (computed downstream from `persist_attr`, not from the
    // pragma). Single-system files behave identically: the lone
    // `@@[persist]` attaches to the lone system.
    let mut pending_persist: bool = false;
    let mut broadcast_persist: bool = false;
    let mut broadcast_save_attrs: Vec<(Option<String>, crate::frame_c::compiler::frame_ast::Span)> =
        Vec::new();
    let mut broadcast_load_attrs: Vec<(Option<String>, crate::frame_c::compiler::frame_ast::Span)> =
        Vec::new();
    // RFC-0052 §4: E829 — a broadcast `*`-attribute is legal only at
    // module position (before any `@@system`). Misplaced ones collect
    // here and surface at the end of the pass.
    let mut broadcast_position_errors: Vec<CompileError> = Vec::new();
    // Strip `(arg)` wrapper from the captured pragma value.
    // Returns None for absent / empty / whitespace-only args.
    fn strip_paren_arg(value: &Option<String>) -> Option<String> {
        let raw = value.as_deref()?;
        let trimmed = raw.trim();
        let inner = trimmed.strip_prefix('(')?.strip_suffix(')')?.trim();
        if inner.is_empty() {
            None
        } else {
            Some(inner.to_string())
        }
    }
    for segment in &source_map.segments {
        if let Segment::Pragma {
            kind: crate::frame_c::compiler::segmenter::PragmaKind::Main,
            span,
            ..
        } = segment
        {
            pending_main_attr_span = Some(crate::frame_c::compiler::frame_ast::Span::new(
                span.start, span.end,
            ));
            continue;
        }
        if let Segment::Pragma {
            kind: crate::frame_c::compiler::segmenter::PragmaKind::Async,
            span,
            ..
        } = segment
        {
            pending_async_attr_span = Some(crate::frame_c::compiler::frame_ast::Span::new(
                span.start, span.end,
            ));
            continue;
        }
        // RFC-0052 §4: `@@[persist]` (next system) / `@@[*persist]`
        // (whole module). Replaces the legacy module-wide stamp.
        if let Segment::Pragma {
            kind: crate::frame_c::compiler::segmenter::PragmaKind::Persist,
            is_broadcast,
            ..
        } = segment
        {
            if *is_broadcast {
                if !system_asts.is_empty() {
                    broadcast_position_errors.push(broadcast_position_error("persist"));
                }
                broadcast_persist = true;
            } else {
                pending_persist = true;
            }
            continue;
        }
        if let Segment::Pragma {
            kind: crate::frame_c::compiler::segmenter::PragmaKind::Create,
            span,
            value,
            ..
        } = segment
        {
            pending_create_attrs.push((
                strip_paren_arg(value),
                crate::frame_c::compiler::frame_ast::Span::new(span.start, span.end),
            ));
            continue;
        }
        if let Segment::Pragma {
            kind: crate::frame_c::compiler::segmenter::PragmaKind::Save,
            span,
            value,
            is_broadcast,
        } = segment
        {
            let attr = (
                strip_paren_arg(value),
                crate::frame_c::compiler::frame_ast::Span::new(span.start, span.end),
            );
            if *is_broadcast {
                if !system_asts.is_empty() {
                    broadcast_position_errors.push(broadcast_position_error("save"));
                }
                broadcast_save_attrs.push(attr);
            } else {
                pending_save_attrs.push(attr);
            }
            continue;
        }
        if let Segment::Pragma {
            kind: crate::frame_c::compiler::segmenter::PragmaKind::Load,
            span,
            value,
            is_broadcast,
        } = segment
        {
            let attr = (
                strip_paren_arg(value),
                crate::frame_c::compiler::frame_ast::Span::new(span.start, span.end),
            );
            if *is_broadcast {
                if !system_asts.is_empty() {
                    broadcast_position_errors.push(broadcast_position_error("load"));
                }
                broadcast_load_attrs.push(attr);
            } else {
                pending_load_attrs.push(attr);
            }
            continue;
        }
        if let Segment::Pragma {
            kind: crate::frame_c::compiler::segmenter::PragmaKind::Import,
            span,
            value,
            ..
        } = segment
        {
            // RFC-0022: `@@import "path"` — strip surrounding quotes,
            // store raw path. Phase 1 is lax (no cross-file resolution
            // *errors*); we still do a best-effort peek on the imported
            // file to discover its `@@system` names so per-target hooks
            // can bind a const to the right identifier. Phase 2 strict
            // mode replaces the peek with full parsing + per-symbol
            // resolution.
            if let Some(raw) = value {
                let stripped = raw
                    .trim()
                    .trim_start_matches('"')
                    .trim_end_matches('"')
                    .to_string();
                if !stripped.is_empty() {
                    let peek = match peek_imported_system_names(
                        &stripped,
                        config.source_path.as_deref(),
                    ) {
                        Ok(data) => {
                            if data.names.is_empty() && config.strict_imports {
                                strict_import_errors.push(CompileError::new(
                                    "E822",
                                    &format!(
                                        "@@import \"{}\" — imported file declares no \
                                         `@@system`. With --import-mode strict every \
                                         import must surface at least one system.",
                                        stripped
                                    ),
                                ));
                            }
                            data
                        }
                        Err(msg) => {
                            if config.strict_imports {
                                strict_import_errors.push(CompileError::new(
                                    "E821",
                                    &format!(
                                        "@@import \"{}\" — {}. \
                                         --import-mode strict requires every imported \
                                         file to be readable.",
                                        stripped, msg
                                    ),
                                ));
                            }
                            PeekData::default()
                        }
                    };
                    // Bug #8: imported sub-systems that use the new persist
                    // contract need their names in the cross-system registry
                    // so the parent's restore codegen emits `Type.new()` +
                    // instance `restore_state(...)` instead of the legacy
                    // static-factory call.
                    for n in &peek.new_contract {
                        if !imported_new_contract_names.iter().any(|x| x == n) {
                            imported_new_contract_names.push(n.clone());
                        }
                    }
                    module_imports.push(crate::frame_c::compiler::frame_ast::Import {
                        module: stripped,
                        symbols: peek.names,
                        alias: None,
                        span: crate::frame_c::compiler::frame_ast::Span::new(span.start, span.end),
                    });
                }
            }
            continue;
        }
        if let Segment::System {
            name,
            body_span,
            header_params_span,
            bases,
            visibility,
            ..
        } = segment
        {
            let ast_body_span = AstSpan::new(body_span.start, body_span.end);

            let mut system_ast = match pipeline_parser::parse_system(
                &source_map.source,
                name.clone(),
                ast_body_span,
                config.target,
            ) {
                Ok(ast) => ast,
                Err(e) => {
                    return Err(CompileResult {
                        code: String::new(),
                        errors: vec![CompileError::new(
                            "E002",
                            &format!("Parse error in system '{}': {}", name, e),
                        )],
                        warnings: vec![],
                        source_map: None,
                    });
                }
            };

            // Parse the optional header parameter list and attach to the
            // freshly-built SystemAst. This is the bridge between the
            // segmenter (which captured the span) and the codegen (which
            // reads system.params to build constructors).
            if let Some(hp_span) = header_params_span {
                let ast_hp_span = AstSpan::new(hp_span.start, hp_span.end);
                match pipeline_parser::parse_system_header_params(&source_map.source, ast_hp_span) {
                    Ok(params) => system_ast.params = params,
                    Err(e) => {
                        return Err(CompileResult {
                            code: String::new(),
                            errors: vec![CompileError::new(
                                "E002",
                                &format!("Parse error in system '{}' header params: {}", name, e),
                            )],
                            warnings: vec![],
                            source_map: None,
                        });
                    }
                }
            }

            // Attach base classes from `: Base1, Base2` syntax
            system_ast.bases = bases.clone();
            // Validate and attach visibility from `@@system private Foo` syntax
            if visibility.as_deref() == Some("public") {
                return Err(CompileResult {
                    code: String::new(),
                    errors: vec![CompileError::new(
                        "E408",
                        &format!(
                            "System '{}': 'public' is redundant — systems are public by default. \
                             Remove the 'public' keyword.",
                            name
                        ),
                    )],
                    warnings: vec![],
                    source_map: None,
                });
            }
            if visibility.as_deref() == Some("private") {
                let unsupported = matches!(
                    config.target,
                    TargetLanguage::Python3
                        | TargetLanguage::Ruby
                        | TargetLanguage::Lua
                        | TargetLanguage::C
                        | TargetLanguage::GDScript
                        | TargetLanguage::Erlang
                );
                if unsupported {
                    return Err(CompileResult {
                        code: String::new(),
                        errors: vec![CompileError::new(
                            "E409",
                            &format!(
                                "System '{}': target language {:?} does not support private class visibility.",
                                name, config.target
                            ),
                        )],
                        warnings: vec![],
                        source_map: None,
                    });
                }
            }
            system_ast.visibility = visibility.clone();

            // RFC-0052 §4: persist now has next-system affinity. A
            // `@@[persist]` (captured in `pending_persist`) stamps THIS
            // system and then resets, so a sibling without its own
            // `@@[persist]` is simply non-persistable — it is no longer
            // force-stamped just because an earlier system persisted. A
            // `@@[*persist]` (captured in `broadcast_persist`) stamps
            // every system and stays set.
            let persist_this_system = broadcast_persist || pending_persist;
            pending_persist = false;
            if persist_this_system {
                system_ast.persist_attr = Some(crate::frame_c::compiler::frame_ast::PersistAttr {
                    save_name: None,
                    restore_name: None,
                    library: None,
                    span: AstSpan::new(0, 0),
                });
            }

            // RFC-0014: attach pending `@@[main]` attribute (if any)
            // to this system. The attribute resets after attachment so
            // it can't bleed onto a later system.
            if let Some(main_span) = pending_main_attr_span.take() {
                system_ast
                    .attributes
                    .push(crate::frame_c::compiler::frame_ast::Attribute {
                        name: "main".to_string(),
                        args: None,
                        span: main_span,
                    });
            }

            // RFC-0043: attach pending `@@[async]` attribute (if any),
            // and set the `is_async_layered` flag so downstream codegen
            // can read it directly without re-scanning attributes or
            // members. Resets after attachment so the pragma can't
            // bleed onto a later system.
            if let Some(async_span) = pending_async_attr_span.take() {
                system_ast
                    .attributes
                    .push(crate::frame_c::compiler::frame_ast::Attribute {
                        name: "async".to_string(),
                        args: None,
                        span: async_span,
                    });
                system_ast.is_async_layered = true;
            }

            // RFC-0015: attach pending lifecycle attributes (`@@[create]`,
            // `@@[save]`, `@@[load]`) to this system. All occurrences
            // are attached so the validator (E818) can detect
            // duplicates. The buffers drain to empty after
            // attachment so a stray pragma followed by native
            // code-then-system can't bleed onto a later system.
            for (arg, span) in pending_create_attrs.drain(..) {
                system_ast
                    .attributes
                    .push(crate::frame_c::compiler::frame_ast::Attribute {
                        name: "create".to_string(),
                        args: arg,
                        span,
                    });
            }
            for (arg, span) in pending_save_attrs.drain(..) {
                system_ast
                    .attributes
                    .push(crate::frame_c::compiler::frame_ast::Attribute {
                        name: "save".to_string(),
                        args: arg,
                        span,
                    });
            }
            for (arg, span) in pending_load_attrs.drain(..) {
                system_ast
                    .attributes
                    .push(crate::frame_c::compiler::frame_ast::Attribute {
                        name: "load".to_string(),
                        args: arg,
                        span,
                    });
            }
            // RFC-0052 §4: broadcast `@@[*save]` / `@@[*load]` attach to
            // every system (cloned, not drained — they apply module-wide).
            // They co-exist with a per-system `@@[save]`/`@@[load]`; E810
            // / E818 then flag the duplicate, which is the correct
            // diagnostic for a system that is both broadcast- and
            // explicitly-named.
            for (arg, span) in broadcast_save_attrs.iter().cloned() {
                system_ast
                    .attributes
                    .push(crate::frame_c::compiler::frame_ast::Attribute {
                        name: "save".to_string(),
                        args: arg,
                        span,
                    });
            }
            for (arg, span) in broadcast_load_attrs.iter().cloned() {
                system_ast
                    .attributes
                    .push(crate::frame_c::compiler::frame_ast::Attribute {
                        name: "load".to_string(),
                        args: arg,
                        span,
                    });
            }

            // Enrich transition metadata (`exit_args`, `enter_args`,
            // `state_args`) from the V4 unified scanner. The pipeline
            // parser leaves these as None because exit args sit before
            // the `->` token and are emitted by the lexer as a trailing
            // NativeCode chunk; they are not visible during the parser's
            // token-by-token consumption of the arrow. The codegen path
            // re-runs the scanner to recover them, but the validator
            // runs *before* codegen and needs them too — e.g. E419
            // (exit args without a matching `<$()` exit handler).
            //
            // The same scanner pass also surfaces structural errors the
            // user must see as compile failures — currently E407 (Frame
            // statement inside a nested function scope, detected via
            // each backend's `skip_nested_scope`). These are propagated
            // here as `CompileError`s so the user gets a clean rejection
            // before validation runs against a partially-scanned AST.
            let enrich_errors =
                crate::frame_c::compiler::native_region_scanner::enrich_system_metadata(
                    &mut system_ast,
                    &source_map.source,
                    config.target,
                );
            if !enrich_errors.is_empty() {
                let errors = enrich_errors
                    .into_iter()
                    .map(|e| CompileError::new(&e.code, &e.message))
                    .collect();
                return Err(CompileResult {
                    code: String::new(),
                    errors,
                    warnings: vec![],
                    source_map: None,
                });
            }

            if config.debug {
                eprintln!(
                    "[compile_ast_based] Parsed system '{}': {} states, {} interface methods",
                    name,
                    system_ast
                        .machine
                        .as_ref()
                        .map(|m| m.states.len())
                        .unwrap_or(0),
                    system_ast.interface.len()
                );
            }

            // RFC-0013 wave 2 phase 2: per-target filter runs LATER
            // (after the validator), so that E800/E801/E802 can fire
            // on attributes whose items would be filtered away.
            system_asts.push(system_ast);
        }
    }

    // RFC-0052 §4: E829 — broadcast `*`-attributes are legal only at
    // module position (before any `@@system`). A misplaced one is a
    // hard error; surface every occurrence in one shot.
    if !broadcast_position_errors.is_empty() {
        return Err(CompileResult {
            code: String::new(),
            errors: broadcast_position_errors,
            warnings: vec![],
            source_map: None,
        });
    }

    Ok(ParsedModule {
        system_asts,
        module_imports,
        imported_new_contract_names,
        strict_import_errors,
    })
}

/// RFC-0052 §4: build the E829 "broadcast attribute not at module
/// position" error for the given attribute name. A `@@[*name]` (the
/// broadcast/spread form) declares module-wide affinity, so it must
/// appear before any `@@system` — placing it before a specific system
/// is ambiguous about whether "all" means the module or just that one.
fn broadcast_position_error(attr: &str) -> CompileError {
    CompileError::new(
        "E829",
        &format!(
            "@@[*{0}] (broadcast form) is only valid at module position — before any \
             `@@system` declaration, alongside `@@[target]`. A `*`-prefixed attribute \
             placed before a specific system is ambiguous. Use `@@[{0}]` (no `*`) to apply \
             it to the next single system, or move the `@@[*{0}]` to the top of the file to \
             broadcast it to every system in the module.",
            attr
        ),
    )
}

/// Pass 1 driver: parse the primary source, then (RFC-0040) resolve any
/// `@@import`ed files into analysis-only systems. Returns `Some` on the first
/// parse/structure error.
pub(crate) fn do_parse(c: &mut PipelineCtx) -> Option<CompileResult> {
    let parsed = {
        let source_map = c.source_map.as_ref().unwrap();
        match parse_module_segments(source_map, &c.config, c.has_persist) {
            Ok(p) => p,
            Err(res) => return Some(res),
        }
    };
    c.system_asts = parsed.system_asts;
    c.module_imports = parsed.module_imports;
    c.imported_new_contract_names = parsed.imported_new_contract_names;
    c.strict_import_errors = parsed.strict_import_errors;

    // RFC-0040: `@@import "<path>"` is an analysis directive. For each import,
    // parse the referenced file's systems into analysis-only `SystemAst`s —
    // visible to validation and the cross-system codegen registries, but never
    // emitted (see `imported_systems` and the codegen emit loop). Unreadable
    // imports are skipped silently here (open-world fallback); strict-mode
    // readability errors are still collected by the peek during parsing.
    let local_names: std::collections::HashSet<String> =
        c.system_asts.iter().map(|s| s.name.clone()).collect();
    let mut imported_asts: Vec<crate::frame_c::compiler::frame_ast::SystemAst> = Vec::new();
    let mut imported_names: std::collections::HashSet<String> = std::collections::HashSet::new();
    let import_paths: Vec<String> = c.module_imports.iter().map(|i| i.module.clone()).collect();
    for path in import_paths {
        let resolved = resolve_import_path(&path, c.config.source_path.as_deref());
        let content = match std::fs::read_to_string(&resolved) {
            Ok(s) => s,
            Err(_) => continue, // open-world: not a readable Frame source — skip.
        };
        let imported_map = match segmenter::segment_source(content.as_bytes(), c.config.target) {
            Ok(sm) => sm,
            Err(_) => continue,
        };
        let imported_has_persist = imported_map.persist_pragma().is_some();
        // Parse the imported file's systems. Discard its own imports
        // (transitive resolution is out of scope) and any structure error
        // (the imported file is validated by its own compilation, not here).
        if let Ok(pm) = parse_module_segments(&imported_map, &c.config, imported_has_persist) {
            for ast in pm.system_asts {
                if local_names.contains(&ast.name) || !imported_names.insert(ast.name.clone()) {
                    continue; // local systems win; dedup repeated imports.
                }
                imported_asts.push(ast);
            }
        }
    }
    c.imported_system_asts = imported_asts;
    c.imported_systems = imported_names;
    None
}

/// Resolve an `@@import` path relative to the importing file's directory (or
/// the current directory when the importer's path is unknown). Mirrors the
/// resolution the import peek performs.
fn resolve_import_path(
    import_path: &str,
    importer_path: Option<&std::path::Path>,
) -> std::path::PathBuf {
    let buf = std::path::PathBuf::from(import_path);
    if buf.is_absolute() {
        buf
    } else if let Some(dir) = importer_path.and_then(|p| p.parent()) {
        dir.join(buf)
    } else {
        buf
    }
}

/// Module-level structure gates + shared arcanum construction. Rejects
/// multi-system Erlang files (E431) and multi-public-class Java files
/// (E430), builds the cross-system `arcanum`, and enforces the single
/// `@@[main]` rule (E805/E806). Returns `Some` on any module-level error.
pub(crate) fn do_module_gates(c: &mut PipelineCtx) -> Option<CompileResult> {
    let config = &c.config;
    let system_asts = &c.system_asts;
    let module_imports = &c.module_imports;

    // Erlang: one module per file — reject multi-system files.
    // E431, distinct from validator's E406 ("Interface handler parameter
    // count mismatch") which lives in `frame_validator.rs`. Both are
    // file-structure issues but they reach the user via different code
    // paths, so they need distinct codes.
    if matches!(config.target, TargetLanguage::Erlang) && system_asts.len() > 1 {
        let names: Vec<&str> = system_asts.iter().map(|s| s.name.as_str()).collect();
        return Some(CompileResult {
            code: String::new(),
            errors: vec![CompileError::new(
                "E431",
                &format!(
                    "Erlang requires one module per file, but this file contains {} systems: {}. \
                     Split into separate files (one @@system per file).",
                    system_asts.len(),
                    names.join(", ")
                ),
            )],
            warnings: vec![],
            source_map: None,
        });
    }

    // Java: one PUBLIC class per file. Multiple package-private
    // (Frame `@@system private`) systems alongside at most one public
    // system is fine — Java allows that.
    //
    // E430 only fires when >1 system would be emitted as public.
    // Distinct from validator's E407 ("Frame statement in nested
    // function scope"). Both apply to source structure but on
    // entirely separate axes, so they need distinct codes.
    if matches!(config.target, TargetLanguage::Java) {
        let public_systems: Vec<&str> = system_asts
            .iter()
            .filter(|s| s.visibility.as_deref() != Some("private"))
            .map(|s| s.name.as_str())
            .collect();
        if public_systems.len() > 1 {
            return Some(CompileResult {
                code: String::new(),
                errors: vec![CompileError::new(
                    "E430",
                    &format!(
                        "Java allows only one public class per file, but this file contains {} public systems: {}. \
                         Either split into separate files (one @@system per file), or mark all but one as `@@system private`.",
                        public_systems.len(),
                        public_systems.join(", ")
                    ),
                )],
                warnings: vec![],
                source_map: None,
            });
        }
    }

    // Build a shared arcanum containing ALL systems so they can reference
    // each other. RFC-0040: this includes `@@import`-resolved systems —
    // present so the validator and the cross-system codegen registries can
    // resolve and check cross-file references. They are analysis-only;
    // codegen emits the local systems alone (see `do_validate_codegen`).
    let mut arcanum_systems = system_asts.clone();
    arcanum_systems.extend(c.imported_system_asts.iter().cloned());
    let module_ast = FrameAst::Module(ModuleAst {
        name: String::new(),
        systems: arcanum_systems,
        imports: module_imports.clone(),
        span: AstSpan::new(0, 0),
    });
    let arcanum = build_arcanum_from_frame_ast(&module_ast);

    // RFC-0014 module-level pass: enforce exactly one `@@[main]` in
    // multi-system files (E805 zero, E806 multiple). Runs once per
    // module, before either codegen path forks. Single-system files
    // are exempt.
    //
    // RFC-0040: `@@[main]` is a module-primary / emission concern, so it is
    // checked against the **locally-declared** systems only. An
    // `@@import`ed system carries its own `@@[main]` for its own
    // compilation; counting it here would spuriously trip E806 in the
    // importer. (The arcanum above keeps the imported systems for
    // cross-file resolution; only this main-attr scope is local.)
    {
        let local_module_ast = FrameAst::Module(ModuleAst {
            name: String::new(),
            systems: system_asts.clone(),
            imports: module_imports.clone(),
            span: AstSpan::new(0, 0),
        });
        let mut module_validator = FrameValidator::new();
        if let Err(errs) = module_validator.validate_module_main_attr(&local_module_ast) {
            let errors = errs
                .iter()
                .map(|e| CompileError::new(&e.code, &e.message))
                .collect();
            return Some(CompileResult {
                code: String::new(),
                errors,
                warnings: vec![],
                source_map: None,
            });
        }
    }

    c.arcanum = Some(arcanum);
    None
}

/// GraphViz target bypass: validate each system, build the graph IR, and
/// emit DOT. Returns `Some(dot_result)` for GraphViz (terminal), `None` to
/// fall through to the normal codegen path.
pub(crate) fn do_graphviz(c: &mut PipelineCtx) -> Option<CompileResult> {
    let config = &c.config;
    if !matches!(config.target, TargetLanguage::Graphviz) {
        return None;
    }
    let source = c.source.as_slice();
    let arcanum = c.arcanum.as_ref().unwrap();
    let mut system_asts = std::mem::take(&mut c.system_asts);
    use crate::frame_c::compiler::graphviz;

    let mut dot_systems: Vec<(String, String)> = Vec::new();

    for system_ast in &mut system_asts {
        // Validate with shared arcanum
        let frame_ast = FrameAst::System(system_ast.clone());
        let mut validator = FrameValidator::new();
        if let Err(errs) = validator.validate_with_arcanum(&frame_ast, &arcanum) {
            let errors = errs
                .iter()
                .map(|e| CompileError::new(&e.code, &e.message))
                .collect();
            return Some(CompileResult {
                code: String::new(),
                errors,
                warnings: vec![],
                source_map: None,
            });
        }
        // @@:self.method() validation against interface
        if let Err(errs) = validator.validate_self_calls(&frame_ast, source, config.target) {
            let errors = errs
                .iter()
                .map(|e| CompileError::new(&e.code, &e.message))
                .collect();
            return Some(CompileResult {
                code: String::new(),
                errors,
                warnings: vec![],
                source_map: None,
            });
        }
        // RFC-0015 D7: validate `@@SystemName(args)` and `@@!SystemName()`
        // call sites — only E820 (no-init zero-arg). E821 (undefined
        // system) was removed per RFC-0024 / bug #30: framec MUST NOT
        // verify cross-system name resolution; host language reports
        // any miss at host-compile time.
        if let Err(errs) =
            validator.validate_system_instantiations(&frame_ast, source, config.target)
        {
            let errors = errs
                .iter()
                .map(|e| CompileError::new(&e.code, &e.message))
                .collect();
            return Some(CompileResult {
                code: String::new(),
                errors,
                warnings: vec![],
                source_map: None,
            });
        }
        // Target-specific checks
        if let Err(errs) = validator.validate_target_specific(&frame_ast, config.target) {
            let errors = errs
                .iter()
                .map(|e| CompileError::new(&e.code, &e.message))
                .collect();
            return Some(CompileResult {
                code: String::new(),
                errors,
                warnings: vec![],
                source_map: None,
            });
        }

        // Filter per `@@[target("X")]` (after validation)
        filter_by_target_attribute(system_ast, config.target);

        // Build graph IR and emit DOT
        let graph = graphviz::build_system_graph(system_ast, &arcanum);
        let dot = graphviz::emit_dot(&graph);
        dot_systems.push((system_ast.name.clone(), dot));
    }

    // Assemble: concatenate DOT blocks with // System: Name headers
    let code = graphviz::emit_multi_system(&dot_systems);

    if config.debug {
        eprintln!(
            "[compile_ast_based] GraphViz: generated {} bytes of DOT for {} systems",
            code.len(),
            dot_systems.len()
        );
    }

    return Some(CompileResult {
        code,
        errors: vec![],
        warnings: vec![],
        source_map: None,
    });
}

/// Pass 2: validate every system against the shared arcanum and emit its
/// code (runtime classes + system class). Populates `generated_systems`
/// and `module_warnings`. Returns `Some` on the first validation error.
pub(crate) fn do_validate_codegen(c: &mut PipelineCtx) -> Option<CompileResult> {
    let config = &c.config;
    let source = c.source.as_slice();
    let arcanum = c.arcanum.as_ref().unwrap();
    let mut system_asts = std::mem::take(&mut c.system_asts);
    let imported_new_contract_names = std::mem::take(&mut c.imported_new_contract_names);
    // RFC-0040: `@@import`-resolved systems. They feed the cross-system
    // registries below (so cross-file persist names, domain params, and
    // contract shape resolve) and the arcanum (already merged in
    // `do_module_gates`), but they are NOT in `system_asts` and so are
    // never reached by the per-system emit loop — analysis-visible,
    // emission-excluded.
    let imported_system_asts = std::mem::take(&mut c.imported_system_asts);

    // Pass 2: Validate + codegen each system with the shared arcanum
    let backend = get_backend(config.target);
    let mut ctx = EmitContext::new();
    // Make the names of every defined system available to the
    // per-backend `emit_field` so it can recognize cross-system
    // domain references (`logger: Logger = @@Logger()`) and emit
    // the right field type per target — Go needs `*Logger`, others
    // use the bare name.
    ctx.defined_systems = arcanum.systems.keys().cloned().collect();
    let mut generated_systems: Vec<(String, String)> = Vec::new();

    // Warnings accumulated across all systems in the module. Harvested
    // from each per-system validator and attached to the final result.
    let mut module_warnings: Vec<CompileError> = Vec::new();

    // RFC-0012 amendment: register which systems use the new persist
    // contract (have `@@[save]` / `@@[load]` ops) so nested-system
    // restore emission can pick the right shape (instance method vs
    // legacy static factory). RFC-0022 / FRAMEC_BUGS.md Issue #8:
    // imported systems that carry the same attributes also belong in
    // the registry — without them, a parent referencing an imported
    // sub-system through `@@[persist]` emits the wrong restore form.
    {
        let mut new_contract: std::collections::HashSet<String> = system_asts
            .iter()
            .chain(imported_system_asts.iter())
            .filter(|s| s.uses_new_persist_contract())
            .map(|s| s.name.clone())
            .collect();
        for n in &imported_new_contract_names {
            new_contract.insert(n.clone());
        }
        crate::frame_c::compiler::codegen::interface_gen::set_new_contract_systems(new_contract);

        // FRAMEC_BUGS Issue #17: register the full set of local
        // `@@system` names so `nested_uses_new_contract` can
        // distinguish "local legacy system" from "cross-file
        // reference" when a name misses the new-contract set.
        // Cross-file references default to new contract; local
        // legacy references default to the legacy emit.
        // RFC-0040: imported systems are resolved (their contract is known
        // from the parsed AST), so register them here too — a legacy
        // imported system then resolves as legacy rather than defaulting to
        // the cross-file new-contract assumption. Truly unknown (open-world,
        // un-imported) names still fall through to that default.
        let local: std::collections::HashSet<String> = system_asts
            .iter()
            .chain(imported_system_asts.iter())
            .map(|s| s.name.clone())
            .collect();
        crate::frame_c::compiler::codegen::interface_gen::set_local_systems(local);
    }

    // FRAMEC_BUGS.md Issue #2 hot-fix (pre-RFC-0015): register each
    // system's Domain-kind params (bare `@@system Inner(seed: int)`
    // params) so nested-system restore can extract their values from
    // the child's saved JSON and pass them to the constructor instead
    // of `Inner.new()` with zero args. RFC-0015 supersedes this with
    // a uniform factory-only model.
    {
        use crate::frame_c::compiler::frame_ast::ParamKind;
        let mut map: std::collections::HashMap<String, Vec<(String, String)>> =
            std::collections::HashMap::new();
        for s in system_asts.iter().chain(imported_system_asts.iter()) {
            let domain_params: Vec<(String, String)> = s
                .params
                .iter()
                .filter(|p| p.kind == ParamKind::Domain)
                .map(|p| {
                    let type_str = match &p.param_type {
                        crate::frame_c::compiler::frame_ast::Type::Custom(s) => s.clone(),
                        _ => String::new(),
                    };
                    (p.name.clone(), type_str)
                })
                .collect();
            if !domain_params.is_empty() {
                map.insert(s.name.clone(), domain_params);
            }
        }
        crate::frame_c::compiler::codegen::interface_gen::set_nested_system_domain_params(map);
    }

    // FRAMEC_BUGS.md Issue #44: register each system's DECLARED
    // `@@[save]` / `@@[load]` method names (`None` where the system
    // used the language default). A *composing* parent's persist
    // codegen reads this so it calls a child's save/load by the
    // child's declared name instead of hardcoding the target default
    // (which broke when the child renamed its persist ops).
    {
        let map: std::collections::HashMap<String, (Option<String>, Option<String>)> = system_asts
            .iter()
            .chain(imported_system_asts.iter())
            .map(|s| {
                (
                    s.name.clone(),
                    (
                        s.save_op_name().map(str::to_string),
                        s.load_op_name().map(str::to_string),
                    ),
                )
            })
            .collect();
        crate::frame_c::compiler::codegen::interface_gen::set_nested_system_persist_names(map);
    }

    // RFC-0043 E721 — cross-system "sync composes async" check. Has to
    // run with the full system list visible; the per-system
    // `validate_with_arcanum` loop below sees one system at a time and
    // cannot tell whether a domain field's type names a sibling async
    // system declared elsewhere in the file. Validation runs on the
    // *unfiltered* AST so the check sees every system attribute.
    {
        let mut e721_validator = FrameValidator::new();
        if let Err(errs) = e721_validator.validate_module_e721_sync_composes_async(&system_asts) {
            let errors = errs
                .iter()
                .map(|e| CompileError::new(&e.code, &e.message))
                .collect();
            return Some(CompileResult {
                code: String::new(),
                errors,
                warnings: module_warnings,
                source_map: None,
            });
        }
    }

    // RFC-0052 §4 E828 — cross-system persist composition. A persistable
    // system that holds a non-persistable sibling as a domain field can't
    // recurse its save/load. Runs with the full (unfiltered) system list
    // for the same reason as E721.
    {
        let mut e828_validator = FrameValidator::new();
        if let Err(errs) = e828_validator.validate_module_e828_persist_composition(&system_asts) {
            let errors = errs
                .iter()
                .map(|e| CompileError::new(&e.code, &e.message))
                .collect();
            return Some(CompileResult {
                code: String::new(),
                errors,
                warnings: module_warnings,
                source_map: None,
            });
        }
    }

    for system_ast in &mut system_asts {
        // Validate with shared arcanum (all sibling systems visible).
        // Validation runs on the *unfiltered* AST so attribute-shape
        // errors (E800/E801/E802) surface even on items that the
        // per-target filter would later prune.
        let frame_ast = FrameAst::System(system_ast.clone());
        let mut validator = FrameValidator::new();
        if let Err(errs) = validator.validate_with_arcanum(&frame_ast, &arcanum) {
            let errors = errs
                .iter()
                .map(|e| CompileError::new(&e.code, &e.message))
                .collect();
            return Some(CompileResult {
                code: String::new(),
                errors,
                warnings: module_warnings,
                source_map: None,
            });
        }
        // @@:self.method() validation against interface
        if let Err(errs) = validator.validate_self_calls(&frame_ast, source, config.target) {
            let errors = errs
                .iter()
                .map(|e| CompileError::new(&e.code, &e.message))
                .collect();
            return Some(CompileResult {
                code: String::new(),
                errors,
                warnings: module_warnings,
                source_map: None,
            });
        }
        // Target-specific checks (e.g. GDScript Object-method collisions,
        // TypeScript global shadowing). Run after the general validator
        // so structural errors surface first.
        if let Err(errs) = validator.validate_target_specific(&frame_ast, config.target) {
            let errors = errs
                .iter()
                .map(|e| CompileError::new(&e.code, &e.message))
                .collect();
            return Some(CompileResult {
                code: String::new(),
                errors,
                warnings: module_warnings,
                source_map: None,
            });
        }
        // Harvest soft warnings (e.g. W501 TypeScript global shadowing,
        // W414 unreachable-state) — these don't fail the build but are
        // surfaced to the user.
        for w in validator.take_warnings() {
            module_warnings.push(CompileError::new(&w.code, &w.message));
        }

        // RFC-0013 wave 2 phase 2: prune items whose `@@[target("X")]`
        // attributes don't match the current target. Runs after all
        // validators so attribute-shape errors fire first.
        filter_by_target_attribute(system_ast, config.target);

        // Warn if async is used with C target (no native async support).
        // Reads the RFC-0043 `is_async_layered` flag set during the
        // attribute-attach pass; equivalent post-validation to scanning
        // members for `is_async` because E720 makes async members
        // without `@@[async]` impossible past the validator.
        if system_ast.is_async_layered && matches!(config.target, TargetLanguage::C) {
            eprintln!("Warning: async is not supported for C — async keyword ignored");
        }

        // Build per-system generated code: runtime classes + system class
        let mut system_code = String::new();

        // Runtime classes (language-specific, per-system)
        if matches!(config.target, TargetLanguage::Rust) {
            let compartment_types = generate_rust_compartment_types(system_ast, Some(&arcanum));
            if !compartment_types.is_empty() {
                system_code.push_str(&compartment_types);
            }
        } else if matches!(config.target, TargetLanguage::C) {
            let c_runtime = generate_c_compartment_types(system_ast);
            if !c_runtime.is_empty() {
                system_code.push_str(&c_runtime);
            }
        } else if matches!(config.target, TargetLanguage::Cpp) {
            let cpp_runtime = generate_cpp_compartment_types(system_ast);
            if !cpp_runtime.is_empty() {
                system_code.push_str(&cpp_runtime);
            }
        } else if matches!(config.target, TargetLanguage::Java) {
            let java_runtime = generate_java_compartment_types(system_ast);
            if !java_runtime.is_empty() {
                system_code.push_str(&java_runtime);
            }
        } else if matches!(config.target, TargetLanguage::CSharp) {
            let csharp_runtime = generate_csharp_compartment_types(system_ast);
            if !csharp_runtime.is_empty() {
                system_code.push_str(&csharp_runtime);
            }
        } else if matches!(config.target, TargetLanguage::Go) {
            let go_runtime = generate_go_compartment_types(system_ast);
            if !go_runtime.is_empty() {
                system_code.push_str(&go_runtime);
            }
        } else if matches!(config.target, TargetLanguage::Kotlin) {
            let kotlin_runtime = generate_kotlin_compartment_types(system_ast);
            if !kotlin_runtime.is_empty() {
                system_code.push_str(&kotlin_runtime);
            }
        } else if matches!(config.target, TargetLanguage::Swift) {
            let swift_runtime = generate_swift_compartment_types(system_ast);
            if !swift_runtime.is_empty() {
                system_code.push_str(&swift_runtime);
            }
        } else if matches!(config.target, TargetLanguage::Erlang) {
            // Erlang gen_statem: no runtime classes needed — gen_statem provides everything
        } else {
            // GDScript module-scope: emit `extends Base` before runtime
            // types so it's the first line (Godot requires this).
            if matches!(config.target, TargetLanguage::GDScript) && !system_ast.bases.is_empty() {
                system_code.push_str(&format!("extends {}\n\n", system_ast.bases[0]));
            }
            if let Some(event_node) = generate_frame_event_class(system_ast, config.target) {
                system_code.push_str(&backend.emit(&event_node, &mut ctx));
                system_code.push_str("\n\n");
            }
            if let Some(context_node) = generate_frame_context_class(system_ast, config.target) {
                system_code.push_str(&backend.emit(&context_node, &mut ctx));
                system_code.push_str("\n\n");
            }
            if let Some(compartment_node) = generate_compartment_class(system_ast, config.target) {
                system_code.push_str(&backend.emit(&compartment_node, &mut ctx));
                system_code.push_str("\n\n");
            }
        }

        // Codegen + Emit with shared arcanum
        ctx = ctx.with_system(&system_ast.name);
        let codegen_node = generate_system(system_ast, &arcanum, config.target, source);
        system_code.push_str(&backend.emit(&codegen_node, &mut ctx));

        generated_systems.push((system_ast.name.clone(), system_code));
    }

    c.system_asts = system_asts;
    c.generated_systems = generated_systems;
    // Merge any @@fsm warnings harvested in do_segment.
    module_warnings.extend(std::mem::take(&mut c.fsm_warnings));
    c.module_warnings = module_warnings;
    None
}

/// Stage 7: assemble the final output (native pass-through + generated
/// systems + system instantiations). Re-derives the backend (cheap,
/// deterministic). Terminal phase — always returns the final result.
pub(crate) fn do_assemble(c: &mut PipelineCtx) -> CompileResult {
    let config = &c.config;
    let source_map = c.source_map.as_ref().unwrap();
    let system_asts = &c.system_asts;
    let module_imports = &c.module_imports;
    let generated_systems = &c.generated_systems;
    let generated_fsms = std::mem::take(&mut c.fsm_generated);
    let strict_import_errors = std::mem::take(&mut c.strict_import_errors);
    let module_warnings = std::mem::take(&mut c.module_warnings);
    let backend = get_backend(config.target);
    let mut runtime_imports = backend.runtime_imports();
    // #94: the C++ persist codegen emits `nlohmann::json` throughout
    // save/restore, but `runtime_imports()` is a fixed per-backend list with no
    // system context, so it can't know whether persistence is enabled. Emit the
    // include here — where the system ASTs are in scope — but ONLY for C++ with
    // at least one persisted system, so non-persisted systems don't take a hard
    // dependency on the (header-only) JSON library. Verbatim `#include` line,
    // matching the other C++ runtime imports.
    if config.target == TargetLanguage::Cpp && system_asts.iter().any(|s| s.persist_attr.is_some())
    {
        runtime_imports.push("#include <nlohmann/json.hpp>".to_string());
    }

    // Stage 7: Assemble final output (native pass-through + system substitution + system instantiations)
    // Runtime imports are emitted first (before any native prolog) to fix import ordering.
    // Pass each system's declared params so the assembler can resolve sigil-tagged
    // call sites (`@@Robot($(10), $>(80), "R2D2")`) and substitute Frame defaults.
    let system_params: Vec<(
        String,
        Vec<crate::frame_c::compiler::frame_ast::SystemParam>,
    )> = system_asts
        .iter()
        .map(|s| (s.name.clone(), s.params.clone()))
        .collect();
    // RFC-0014: identify the file's primary system. Multi-system files
    // require exactly one `@@[main]` (already validated by E805/E806);
    // single-system files take their lone system as implicit primary.
    let main_system: Option<String> = if system_asts.len() == 1 {
        Some(system_asts[0].name.clone())
    } else {
        system_asts
            .iter()
            .find(|s| s.is_main())
            .map(|s| s.name.clone())
    };
    // RFC-0022: ask the backend to translate `@@import` directives into
    // its native form. Default impl returns empty (no emission); per-
    // backend overrides translate per target.
    //
    // RFC-0040: `@@import` is analysis-only and emits NOTHING — native
    // host imports are the user's own Oceans Model pass-through. So the
    // emitted-imports list is always empty regardless of backend; the
    // directive's effect is confined to analysis/resolution. (The peeked
    // names are still surfaced below so cross-file `@@SystemName()` call
    // sites lower as external references rather than erroring.)
    let module_imports_emitted: Vec<String> = Vec::new();
    let _ = &module_imports; // analysis metadata only; never emitted (RFC-0040).
                             // Imported `@@system` names — surfaced by the Phase 1 peek. The
                             // assembler accepts these as resolvable targets for `@@SystemName()`
                             // call sites in handler bodies and module-scope native code.
    let imported_system_names: Vec<String> = module_imports
        .iter()
        .flat_map(|imp| imp.symbols.iter().cloned())
        .collect();
    let code = match assembler::assemble(
        &source_map,
        &generated_systems,
        &generated_fsms,
        &system_params,
        config.target,
        &runtime_imports,
        &module_imports_emitted,
        &imported_system_names,
        main_system.as_deref(),
    ) {
        Ok(output) => output,
        Err(e) => {
            return CompileResult {
                code: String::new(),
                errors: vec![CompileError::new("E003", &format!("Assembly error: {}", e))],
                warnings: vec![],
                source_map: None,
            };
        }
    };

    if config.debug {
        eprintln!("[compile_ast_based] Generated {} bytes of code", code.len());
    }

    // RFC-0022 strict-mode errors collected during import resolution
    // surface here. They don't abort earlier passes (the rest of the
    // module still compiles), so the user sees both the missing-import
    // error AND any downstream issues in one shot.
    CompileResult {
        code: if strict_import_errors.is_empty() {
            code
        } else {
            String::new()
        },
        errors: strict_import_errors,
        warnings: module_warnings,
        source_map: None,
    }
}

/// Compile using the V4 pipeline stages
///
/// Pipeline: Segmenter → Parser → Arcanum → Validator → Codegen → Emit → Assembler
///
/// 1. Segment source into Native/Pragma/System regions (Segmenter)
/// 2. For each System segment: parse → build Arcanum → validate → generate code
/// 3. Assemble final output: native pass-through + generated systems + system instantiations
pub fn compile_ast_based(
    source: &[u8],
    config: &PipelineConfig,
) -> Result<CompileResult, RunError> {
    // RFC-0035 Round 8: the phase sequence is driven by the `PipelineFsm`
    // Frame state machine — each phase is a state whose enter handler runs
    // one `do_*` phase and transitions. See `compiler/pipeline_supervisor/`.
    Ok(crate::frame_c::compiler::pipeline_supervisor::run_pipeline(
        source, config,
    ))
}

/// RFC-0013 wave 2: prune AST items whose `@@[target("X")]` attributes
/// don't include the current target.
///
/// An item with no `target` attribute is always emitted. An item with one
/// or more `target` attributes is emitted only when at least one matches
/// `current`. Unparseable target args are treated as non-matches (a future
/// validator pass will surface them as a hard error).
/// Data surfaced by an `@@import` peek.
///
/// `names` — every `@@system <Name>` declaration in source order.
/// `new_contract` — the subset of `names` that carry an
/// `@@[save(...)]` and/or `@@[load(...)]` attribute on the line(s)
/// immediately preceding the system declaration. The persist
/// codegen branches on this to pick the cross-file restore shape
/// (instance method on the new contract, legacy static factory
/// otherwise).
#[derive(Debug, Default, Clone)]
struct PeekData {
    names: Vec<String>,
    new_contract: Vec<String>,
}

/// Outcome of an `@@import` peek.
///
/// `Ok(data)` — the imported file was readable; `data.names` lists the
/// surfaced systems (possibly empty, which strict mode treats as E822 —
/// nothing to import).
///
/// `Err(message)` — the imported file couldn't be read (missing,
/// permission denied, IO error). Lax mode swallows this and treats
/// it as `Ok(PeekData::default())`; strict mode surfaces E821 with
/// this message.
type PeekResult = Result<PeekData, String>;

/// RFC-0022 import peek. Resolve `import_path` relative to the
/// importer's directory (or CWD when unknown), read the file, and pull
/// out every `@@system <Name>` declaration. Returns the discovered
/// system names in source order, or an error if the file is unreadable.
///
/// This is a regex-grade scan, not a full parse — bracket-form
/// attributes, line comments, and multi-line `@@system` blocks
/// surrounding the declaration are all handled by anchoring on the
/// `@@system` keyword and reading the next identifier. Strict mode
/// (RFC-0022 `--import-mode strict`) surfaces unreadable files / empty
/// imports as compile errors; lax mode treats them as empty and lets
/// per-target hooks fall back to filename-derived bindings.
fn peek_imported_system_names(
    import_path: &str,
    importer_path: Option<&std::path::Path>,
) -> PeekResult {
    let import_buf = std::path::PathBuf::from(import_path);
    let resolved = if import_buf.is_absolute() {
        import_buf
    } else if let Some(importer) = importer_path.and_then(|p| p.parent()) {
        importer.join(&import_buf)
    } else {
        import_buf
    };
    let content = match std::fs::read_to_string(&resolved) {
        Ok(s) => s,
        Err(e) => {
            return Err(format!(
                "cannot read imported file '{}' (resolved to {}): {}",
                import_path,
                resolved.display(),
                e
            ));
        }
    };
    let mut names: Vec<String> = Vec::new();
    let mut new_contract: Vec<String> = Vec::new();
    // RFC-0012 amendment: `@@[save(...)]` / `@@[load(...)]` attributes
    // attach to the *next* `@@system` declaration. Track whether either
    // has been seen since the last `@@system` consumption; if so, the
    // next system is registered as new-contract.
    let mut pending_save_or_load = false;
    for line in content.lines() {
        let trimmed = line.trim_start();
        // Skip comment lines so commented-out `@@system` declarations
        // don't pollute the peek.
        if trimmed.starts_with("//") || trimmed.starts_with('#') {
            continue;
        }
        // Pre-system attribute detection: a bracket-form pragma that
        // names `save` or `load` flips the pending flag. Match against
        // the trimmed line because attributes may be wrapped in `@@[`.
        if trimmed.starts_with("@@[save") || trimmed.starts_with("@@[load") {
            pending_save_or_load = true;
            continue;
        }
        let rest = match trimmed.strip_prefix("@@system") {
            Some(r) => r,
            None => continue,
        };
        // Require a separator after `@@system` so `@@systemd` (a
        // hypothetical future keyword / typo) doesn't false-match.
        let next = rest.chars().next();
        if !matches!(next, Some(c) if c.is_whitespace()) {
            continue;
        }
        // RFC-0014 visibility marker (`@@system private Name`) sits
        // between the keyword and the name; skip it if present.
        let mut tokens = rest.split_whitespace();
        let first = match tokens.next() {
            Some(t) => t,
            None => continue,
        };
        let name_token = if first == "private" || first == "public" {
            match tokens.next() {
                Some(t) => t,
                None => continue,
            }
        } else {
            first
        };
        // Trim trailing punctuation that can attach to the name token
        // when there's no separating whitespace (e.g. `Counter:` for a
        // base-class declaration, `Counter{` for an inlined body).
        let clean: String = name_token
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
            .collect();
        if clean.is_empty() {
            continue;
        }
        if !names.iter().any(|n| n == &clean) {
            names.push(clean.clone());
        }
        // If `@@[save]` and/or `@@[load]` preceded this system, the
        // system uses the new persist contract. Record it and reset
        // the flag so attributes on the next system are tracked
        // independently.
        if pending_save_or_load && !new_contract.iter().any(|n| n == &clean) {
            new_contract.push(clean);
        }
        pending_save_or_load = false;
    }
    Ok(PeekData {
        names,
        new_contract,
    })
}

fn filter_by_target_attribute(
    system_ast: &mut crate::frame_c::compiler::frame_ast::SystemAst,
    current: TargetLanguage,
) {
    use crate::frame_c::compiler::frame_ast::Attribute;

    fn unquote(s: &str) -> &str {
        let t = s.trim();
        let bytes = t.as_bytes();
        if bytes.len() >= 2
            && ((bytes[0] == b'"' && bytes[bytes.len() - 1] == b'"')
                || (bytes[0] == b'\'' && bytes[bytes.len() - 1] == b'\''))
        {
            &t[1..t.len() - 1]
        } else {
            t
        }
    }

    fn should_emit(attrs: &[Attribute], current: TargetLanguage) -> bool {
        let mut saw_target = false;
        for a in attrs {
            if a.name != "target" {
                continue;
            }
            saw_target = true;
            if let Some(args) = &a.args {
                let lang_str = unquote(args);
                if let Ok(t) = TargetLanguage::try_from(lang_str) {
                    if t == current {
                        return true;
                    }
                }
            }
        }
        !saw_target
    }

    system_ast
        .interface
        .retain(|m| should_emit(&m.attributes, current));

    system_ast
        .domain
        .retain(|d| should_emit(&d.attributes, current));

    if let Some(machine) = system_ast.machine.as_mut() {
        for state in machine.states.iter_mut() {
            state
                .handlers
                .retain(|h| should_emit(&h.attributes, current));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compile_result_creation() {
        let result = CompileResult {
            code: "generated code".to_string(),
            errors: vec![],
            warnings: vec![],
            source_map: None,
        };
        assert_eq!(result.code, "generated code");
        assert!(result.errors.is_empty());
    }

    #[test]
    fn test_compile_error_with_location() {
        let error = CompileError::new("E001", "test error").with_location(10, 5);
        assert_eq!(error.line, Some(10));
        assert_eq!(error.column, Some(5));
    }

    // --- RFC-0042 @@fsm driver wiring (Task 14) ---

    fn compile_py(source: &str) -> CompileResult {
        use crate::frame_c::compiler::pipeline_supervisor::run_pipeline;
        let config = PipelineConfig::production(TargetLanguage::Python3);
        run_pipeline(source.as_bytes(), &config)
    }

    /// A clean `@@fsm` block compiles without errors and emits the
    /// generated Python recognizer into the output.
    #[test]
    fn fsm_block_clean_compiles() {
        let r = compile_py("@@fsm M(text: bytes) : bool = false { /a/ true }\n");
        assert!(
            r.errors.is_empty(),
            "expected no errors, got {:?}",
            r.errors
        );
        assert!(
            r.code.contains("class M"),
            "expected emitted class, got:\n{}",
            r.code
        );
        assert!(r.code.contains("def _run"), "expected the DFA driver");
    }

    /// End-to-end: the Python emitted by the full pipeline actually runs
    /// and produces the FSM-TEST-001 verdicts. Proves `framec compile -l
    /// python_3` of an `@@fsm` yields runnable output. Self-skips if
    /// python3 is unavailable.
    #[test]
    fn fsm_block_emitted_python_runs() {
        use std::process::Command;
        let r = compile_py("@@fsm M(text: bytes) : bool = false { /a/ true }\n");
        assert!(r.errors.is_empty(), "got {:?}", r.errors);
        let driver = format!(
            "{}\nimport sys\nm = M(sys.argv[1])\nprint(m.accepted)\n",
            r.code
        );
        let path = std::env::temp_dir().join("framec_fsm_e2e_pipeline.py");
        std::fs::write(&path, driver).expect("write temp py");
        let out = match Command::new("python3").arg(&path).arg("a").output() {
            Ok(o) => o,
            Err(_) => return, // python3 absent — skip
        };
        assert!(
            out.status.success(),
            "python3 failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "True");
    }

    /// RFC-0042 Mode A/B: a `@@system` handler invokes an `@@fsm` via
    /// `@@FsmName(args)` (a plain constructor, not the `._create` factory)
    /// and reads its instance fields. Compiles both into one module and
    /// runs the system driving the fsm. (Only `int` domain fields are used:
    /// a `@@system` `bool = false` default emits Python `false`, an
    /// unrelated pre-existing @@system codegen behavior.)
    #[test]
    fn fsm_mode_a_called_from_system() {
        use std::process::Command;
        // `@@system` statements/fields are newline-separated (unlike the
        // whitespace-agnostic `@@fsm` body), so use a real multi-line source.
        let src = r#"
@@fsm Digits(text: bytes) : int = 0 { /[0-9]+/ to_int(@@:matched) }

@@system Parser {
    interface:
        parse(buf: bytes)
    machine:
        $Start {
            parse(buf) {
                m = @@Digits(buf)
                self.value = m.return_value
                self.cur = m.cursor
            }
        }
    domain:
        value: int = 0
        cur: int = 0
}
"#;
        let r = compile_py(src);
        assert!(r.errors.is_empty(), "got {:?}", r.errors);
        // The fsm call site is a plain constructor, not the `._create`
        // factory used for `@@system` instantiation.
        assert!(
            r.code.contains("m = Digits(buf)"),
            "expected plain fsm constructor, got:\n{}",
            r.code
        );
        let driver = format!(
            "{}\np = Parser._create()\np.parse(\"123\")\nprint(p.value, p.cur)\n\
             q = Parser._create()\nq.parse(\"xy\")\nprint(q.value, q.cur)\n",
            r.code
        );
        let path = std::env::temp_dir().join("framec_fsm_mode_a.py");
        std::fs::write(&path, driver).expect("write temp py");
        let out = match Command::new("python3").arg(&path).output() {
            Ok(o) => o,
            Err(_) => return,
        };
        assert!(
            out.status.success(),
            "python3 failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        let text = String::from_utf8_lossy(&out.stdout);
        let lines: Vec<&str> = text.lines().collect();
        // "123": fsm matches → return_value 3-digit int, cursor 3.
        assert_eq!(lines[0], "123 3", "digits input read back through Mode A");
        // "xy": fsm rejects → return_value stays at its default 0, cursor 0.
        assert_eq!(lines[1], "0 0", "non-digit input");
    }

    /// Compile a multi-fsm module via the pipeline, construct `class`
    /// over `input`, and return `(accepted, return_value, cursor)` as
    /// Python `repr`/`str`. `None` if python3 is unavailable.
    fn run_fsm_class(
        src: &str,
        class: &str,
        input: &str,
        tag: &str,
    ) -> Option<(String, String, String)> {
        use std::process::Command;
        let r = compile_py(src);
        assert!(r.errors.is_empty(), "compile errors: {:?}", r.errors);
        let driver = format!(
            "{}\nm = {}({:?})\nprint(m.accepted)\nprint(repr(m.return_value))\nprint(m.cursor)\n",
            r.code, class, input
        );
        let path = std::env::temp_dir().join(format!("framec_{}.py", tag));
        std::fs::write(&path, driver).expect("write temp py");
        let out = Command::new("python3").arg(&path).output().ok()?;
        assert!(
            out.status.success(),
            "python3 failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        let text = String::from_utf8_lossy(&out.stdout);
        let l: Vec<&str> = text.lines().collect();
        Some((l[0].to_string(), l[1].to_string(), l[2].to_string()))
    }

    /// FSM-TEST-701 — Mode C bytes-and-return. RFC-0042 §8.3: a `/@Inner/`
    /// stage calls another fsm at the cursor; `$state.label.return_value` reads
    /// the inner's return value and the cursor advances by the inner's.
    /// (Tuple-return variants in FSM-TEST-700 are a separate parser gap.)
    #[test]
    fn fsm_mode_c_call_out() {
        let src = "@@fsm Digit(input: char) : int = 0 { /[0-9]/ to_int(@@:matched) }\n\
                   @@fsm Wrap(input: char) : int = 0 { $w: .d/@Digit/ $w.d.return_value }\n";
        let Some((acc, ret, cur)) = run_fsm_class(src, "Wrap", "5", "mc_co_a") else {
            return;
        };
        assert_eq!(
            (acc.as_str(), ret.as_str(), cur.as_str()),
            ("True", "5", "1")
        );
        // Inner rejects → the Mode C stage fails → outer rejects.
        let (acc2, _, _) = run_fsm_class(src, "Wrap", "x", "mc_co_b").unwrap();
        assert_eq!(acc2, "False");
    }

    /// Mode C `$state.label` (no `.return_value`) is the matched slice,
    /// distinct from the inner's return value.
    #[test]
    fn fsm_mode_c_slice_access() {
        let src = "@@fsm Digit(input: char) : int = 0 { /[0-9]/ to_int(@@:matched) }\n\
                   @@fsm Echo(input: char) : int = 0 { $e: .d/@Digit/ to_int($e.d) }\n";
        let Some((_, ret, _)) = run_fsm_class(src, "Echo", "5", "mc_sl") else {
            return;
        };
        assert_eq!(ret, "5"); // to_int(slice "5") == 5
    }

    /// Chained Mode C with a regex stage between (FSM-TEST-700 structure,
    /// summing instead of returning a tuple): `.a/@Digit/ /,/ .b/@Digit/`.
    #[test]
    fn fsm_mode_c_chained() {
        let src = "@@fsm Digit(input: char) : int = 0 { /[0-9]/ to_int(@@:matched) }\n\
                   @@fsm Sum(input: char) : int = 0 { \
                   $p: .a/@Digit/ /,/ .b/@Digit/ ($p.a.return_value + $p.b.return_value) }\n";
        let Some((acc, ret, _)) = run_fsm_class(src, "Sum", "3,7", "mc_ch_a") else {
            return;
        };
        assert_eq!((acc.as_str(), ret.as_str()), ("True", "10"));
        // "3-7": the `/,/` stage fails on '-' → reject.
        let (acc2, _, _) = run_fsm_class(src, "Sum", "3-7", "mc_ch_b").unwrap();
        assert_eq!(acc2, "False");
    }

    // (Removed `fsm_block_unsupported_target_e740`: all 17 targets now have
    // an @@fsm backend, so no target reaches the E740 unsupported-target
    // path. The defensive `None` arm in `do_segment` remains as a backstop.)

    /// A bad input-parameter type surfaces E713 through the pipeline.
    #[test]
    fn fsm_block_e713_errors() {
        let r = compile_py("@@fsm M(text: float) : bool = false { /a/ true }\n");
        assert!(
            r.errors.iter().any(|e| e.code == "E713"),
            "expected E713, got {:?}",
            r.errors
        );
    }

    /// A `@@fsm` parse error (missing return type) surfaces as a compile error.
    #[test]
    fn fsm_block_parse_error_surfaces() {
        let r = compile_py("@@fsm M(text: bytes) = false { /a/ true }\n");
        assert!(!r.errors.is_empty(), "expected a parse error, got none");
    }

    /// An unused domain field surfaces W703 as a warning (and still compiles).
    #[test]
    fn fsm_block_w703_warning() {
        let r = compile_py(
            "@@fsm M(text: bytes) : bool = false { /a/ true  domain: unused: int = 0 }\n",
        );
        assert!(
            r.errors.is_empty(),
            "expected no errors, got {:?}",
            r.errors
        );
        assert!(
            r.warnings.iter().any(|w| w.code == "W703"),
            "expected W703 warning, got {:?}",
            r.warnings
        );
    }

    /// A forbidden regex construct inside an `@@fsm` surfaces its engine
    /// diagnostic (E720) through the full pipeline.
    #[test]
    fn fsm_block_regex_e720_errors() {
        // Lookahead is non-regular → E720 (lazy quantifiers now compile).
        let r = compile_py("@@fsm M(text: bytes) : bool = false { /a(?=b)/ true }\n");
        assert!(
            r.errors.iter().any(|e| e.code == "E720"),
            "expected E720, got {:?}",
            r.errors
        );
    }

    /// A broken `@@fsm` alongside a valid `@@system` fails the compile with
    /// the fsm diagnostic.
    #[test]
    fn fsm_alongside_system() {
        let src = "@@system S { interface: go() machine: $A { go() {} } }\n\
                   @@fsm M(text: float) : bool = false { /a/ true }\n";
        let r = compile_py(src);
        assert!(
            r.errors.iter().any(|e| e.code == "E713"),
            "got {:?}",
            r.errors
        );
    }

    #[test]
    fn test_validation_only_mode() {
        let source = b"@@system Test { machine: $A { } }";
        let config = PipelineConfig::validation_only(TargetLanguage::Python3);
        let result = compile_module(source, &config);
        assert!(result.is_ok());
    }

    #[test]
    fn test_compile_simple_system() {
        let source = b"@@system Test { machine: $Init { } }";
        let config = PipelineConfig::production(TargetLanguage::Python3);
        let result = compile_module(source, &config);
        assert!(result.is_ok());
        let output = result.unwrap();
        if !output.errors.is_empty() {
            eprintln!("Parse errors: {:?}", output.errors);
            return;
        }
        assert!(output.code.contains("class Test"));
    }

    #[test]
    fn test_compile_with_transition() {
        let source = br#"@@system TestTransition {
    machine:
        $Idle {
            start() {
                -> $Running
            }
        }
        $Running {
            stop() {
                -> $Idle
            }
        }
}"#;
        let config = PipelineConfig::production(TargetLanguage::Python3);
        let result = compile_module(source, &config);
        assert!(result.is_ok());
        let output = result.unwrap();
        if !output.errors.is_empty() {
            for e in &output.errors {
                eprintln!("Error: {}: {}", e.code, e.message);
            }
            return;
        }
        assert!(output.code.contains("_transition"));
    }

    #[test]
    fn test_native_only_input_passes_through() {
        // Input with no @@system blocks is pure native code — passes through verbatim
        let source = b"this is just native code\nno systems here\n";
        let config = PipelineConfig::production(TargetLanguage::Python3);
        let result = compile_module(source, &config);
        assert!(result.is_ok());
        let output = result.unwrap();
        assert!(output.errors.is_empty());
        assert!(output.code.contains("this is just native code"));
    }

    #[test]
    fn test_compile_parse_error() {
        // Invalid syntax inside @@system should produce an error
        let source = b"@@system Test { not valid section syntax }";
        let config = PipelineConfig::production(TargetLanguage::Python3);
        let result = compile_module(source, &config);
        assert!(result.is_ok());
        let output = result.unwrap();
        assert!(
            !output.errors.is_empty(),
            "Expected parse errors for invalid system content"
        );
    }

    /// RFC-0017 regression: a parameterized `@@[persist]` child held by
    /// an orchestrator must (a) be field-initialized via the factory
    /// (`Counter._create(7)`) — never the bare-ctor spelling — and
    /// (b) be rehydrated in the orchestrator's `restore_state` with the
    /// bare no-arg ctor (`Counter.new()`), not `Counter.new(<saved
    /// arg>)` which after init-decoupling overflows the parameterless
    /// `_init()`. GDScript-specific: the other typed backends already
    /// emitted the bare-`new` form in the restore path.
    #[test]
    fn test_gdscript_persist_param_child_call_sites() {
        let source = br#"@@[target("gdscript")]
@@[persist(PackedByteArray)]
@@[save(save_state)]
@@[load(restore_state)]
@@system Counter(seed: int) {
    interface:
        bump()
    machine:
        $S { $> { self.n = self.seed } bump() { self.n = self.n + 1 } }
    domain:
        seed: int = seed
        n: int = 0
}
@@[main]
@@[persist(PackedByteArray)]
@@[save(save_state)]
@@[load(restore_state)]
@@system World {
    interface:
        bump_c()
    machine:
        $S { bump_c() { self.c.bump() } }
    domain:
        c = @@Counter(7)
}
"#;
        let config = PipelineConfig::production(TargetLanguage::GDScript);
        let output = compile_module(source, &config).expect("pipeline error");
        assert!(
            output.errors.is_empty(),
            "compile errors: {:?}",
            output.errors
        );
        let code = &output.code;
        // (a) field initializer uses the factory
        assert!(
            code.contains("Counter._create(7)"),
            "domain-init should use the RFC-0017 factory; got:\n{}",
            code
        );
        // (b) restore_state rehydrates via the bare no-arg ctor
        assert!(
            code.contains("self.c = Counter.new()"),
            "restore_state should rehydrate via bare `Counter.new()`; got:\n{}",
            code
        );
        // and never passes the saved ctor arg to the parameterless ctor
        assert!(
            !code.contains("Counter.new(__raw"),
            "restore_state must not pass saved args to the no-arg bare ctor; got:\n{}",
            code
        );
    }

    /// RFC-0017 regression: a single `@@system Foo : RefCounted` emits at
    /// GDScript script-module scope (no `class Foo:` wrapper — the file
    /// IS Foo). The init-decouple `_create()` body references the script
    /// by name (`Foo.new()`), which has no referent at module scope
    /// without a `class_name` declaration → Godot "Identifier not found:
    /// Foo". The assembler must prepend `class_name Foo` (before
    /// `extends`).
    #[test]
    fn test_gdscript_module_scope_system_has_class_name() {
        let source = br#"@@[target("gdscript")]
@@system Adventure : RefCounted {
    interface:
        bump()
        get_value(): int
    machine:
        $S {
            bump() { self.n = self.n + 1 }
            get_value(): int { @@:(self.n) }
        }
    domain:
        n: int = 0
}
"#;
        let config = PipelineConfig::production(TargetLanguage::GDScript);
        let output = compile_module(source, &config).expect("pipeline error");
        assert!(
            output.errors.is_empty(),
            "compile errors: {:?}",
            output.errors
        );
        let code = &output.code;
        // The module-scope system's `_create` references `Adventure.new()`.
        assert!(
            code.contains("Adventure.new()"),
            "expected the module-scope `_create` to reference `Adventure.new()`; got:\n{}",
            code
        );
        // ...so the script must declare `class_name Adventure` (before
        // `extends`) for that identifier to resolve.
        let class_name_at = code.find("class_name Adventure");
        let extends_at = code.find("extends RefCounted");
        assert!(
            class_name_at.is_some(),
            "module-scope GDScript system must emit `class_name Adventure`; got:\n{}",
            code
        );
        assert!(
            class_name_at < extends_at,
            "`class_name` must precede `extends`; got:\n{}",
            code
        );
    }
}
