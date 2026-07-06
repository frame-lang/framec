//! Output Assembler (Stage 7 of the V4 Pipeline)
//!
//! Takes the `SourceMap` from the Segmenter (Stage 0) and generated code from
//! the Codegen/Emit stages (Stages 5-6), and produces the final output file.
//!
//! Algorithm:
//! 1. Walk `SourceMap.segments` in order:
//!    - `Segment::Native` → extract text from source bytes at span, append to output
//!    - `Segment::Pragma` → skip (consumed by earlier stages)
//!    - `Segment::System` → look up system name in generated_systems, append generated code
//! 2. Post-process: expand `@@SystemName()` system instantiations in native regions
//! 3. Return final assembled output

use crate::frame_c::compiler::codegen::codegen_utils::to_snake_case;
use crate::frame_c::compiler::frame_ast::SystemParam;
use crate::frame_c::compiler::native_region_scanner::create_skipper;
use crate::frame_c::compiler::native_region_scanner::unified::SyntaxSkipper;
use crate::frame_c::compiler::pipeline_parser::call_args::{
    parse_call_args, resolve_call, CallArgsError,
};
use crate::frame_c::compiler::segmenter::{Segment, SourceMap};
use crate::frame_c::visitors::TargetLanguage;
use std::collections::HashMap;
use std::collections::HashSet;

// ============================================================================
// Assembly Error
// ============================================================================

#[derive(Debug, Clone)]
pub struct AssemblyError {
    pub message: String,
}

impl std::fmt::Display for AssemblyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Assembly error: {}", self.message)
    }
}

impl std::error::Error for AssemblyError {}

// ============================================================================
// Public API
// ============================================================================

/// Assemble the final output from source map and generated system code.
///
/// `source_map` — the segmented source from Stage 0
/// `generated_systems` — Vec of (system_name, generated_code) from Stages 5-6
/// `system_params` — Vec of (system_name, declared params) so the assembler
///   can resolve `@@SystemName(args)` call sites against the declared shape
///   (sigil checks, named lookup, default substitution).
/// `lang` — target language for system instantiation expansion
/// `runtime_imports` — imports required by generated code (emitted before any native code)
/// `main_system` — RFC-0014 primary system. For multi-system GDScript files this
///   is the system whose code emits at script-module scope; every other system
///   wraps as an inner class. None for single-system files (no special handling
///   needed) or for non-GDScript targets where the attribute is metadata-only.
pub fn assemble(
    source_map: &SourceMap,
    generated_systems: &[(String, String)],
    generated_fsms: &[(String, String)],
    system_params: &[(String, Vec<SystemParam>)],
    lang: TargetLanguage,
    runtime_imports: &[String],
    module_imports: &[String],
    imported_system_names: &[String],
    main_system: Option<&str>,
    // #171: Go persist emits `json.Marshal`/`json.Unmarshal`; when set, inject
    // `import "encoding/json"` after the `package` clause (deduped).
    go_needs_json_import: bool,
) -> Result<String, AssemblyError> {
    let source = &source_map.source;
    let mut output = String::new();

    // Emit runtime imports first (before any native prolog code)
    // This ensures imports like "from typing import ..." come before user code
    for import in runtime_imports {
        output.push_str(import);
        output.push('\n');
    }
    if !runtime_imports.is_empty() {
        output.push('\n');
    }

    // RFC-0022 — emit module-scope `@@import` translations after the
    // runtime imports but before the user prolog. Each backend's
    // `emit_module_imports` produces the native form.
    //
    // GDScript is special-cased: `class_name` and `extends` must lead
    // the file (and `class_name` must precede `extends`). Const-level
    // `preload(...)` bindings are statements and must appear *after*
    // those header lines. We defer the GDScript module-imports
    // emission until after the class_name/extends header block below.
    let is_gdscript = matches!(lang, TargetLanguage::GDScript);
    if !is_gdscript {
        for import in module_imports {
            output.push_str(import);
            output.push('\n');
        }
        if !module_imports.is_empty() {
            output.push('\n');
        }
    }

    // Build lookup for generated systems
    let system_code: HashMap<&str, &str> = generated_systems
        .iter()
        .map(|(name, code)| (name.as_str(), code.as_str()))
        .collect();

    // The post-pass resolver accepts both systems defined in this file
    // AND systems brought in via `@@import` (peek-surfaced names).
    // Imported systems lack local `system_params` entries, so the
    // resolver skips default-arg substitution for them and passes the
    // call arguments through verbatim.
    let mut defined_system_names: HashSet<String> = generated_systems
        .iter()
        .map(|(name, _)| name.clone())
        .collect();
    for sym in imported_system_names {
        defined_system_names.insert(sym.clone());
    }

    // Build name → declared-params lookup for call-site resolution
    let params_by_name: HashMap<&str, &[SystemParam]> = system_params
        .iter()
        .map(|(name, params)| (name.as_str(), params.as_slice()))
        .collect();

    // RFC-0042 Mode A/B: `@@FsmName(args)` call sites resolve to a plain
    // fsm constructor, distinct from a `@@system`'s `._create` factory.
    let fsm_names: HashSet<&str> = generated_fsms.iter().map(|(n, _)| n.as_str()).collect();

    // GDScript multi-system: the system whose name matches
    // `main_system` (RFC-0014's `@@[main]`) emits at script-module
    // scope; every other system wraps as a sibling inner class.
    // `main_system` is None for single-system files (no special
    // handling needed — the lone system is implicitly primary) and
    // for non-GDScript targets.
    //
    // GDScript additionally requires the script-level `extends Base`
    // directive to appear before any other declaration (after
    // optional `class_name`). The main system's per-system emission
    // begins with its `extends Base` line, but in source order the
    // main system is typically NOT first (frame-arcade convention:
    // primitives first, composer last). We hoist the main system's
    // `extends` line to the top of the file and strip it from the
    // main system's emission during the walk so the script parses
    // cleanly.
    let main_extends_line: Option<String> = if is_gdscript {
        main_system.and_then(|m| {
            generated_systems
                .iter()
                .find(|(name, _)| name == m)
                .and_then(|(_, code)| extract_leading_extends_line(code))
        })
    } else {
        None
    };
    // When the main system emits at script-module scope (it has an
    // explicit `extends Base` — codegen put it here, not in a `class`
    // wrapper), the RFC-0017 `_create` factory body references the
    // script by name (`Adventure.new()`). A module-scope GDScript
    // script has no implicit self-identifier, so declare `class_name`
    // (which must precede `extends`) to make the script resolvable as
    // a global identifier.
    if is_gdscript {
        if let (Some(m), Some(_)) = (main_system, &main_extends_line) {
            output.push_str(&format!("class_name {}\n", m));
        }
    }
    if let Some(ref ext_line) = main_extends_line {
        output.push_str(ext_line);
        output.push_str("\n\n");
    }
    // Now emit GDScript module-imports (deferred above) — placement is
    // after class_name + extends, before any user prolog or system
    // emission. `const X = preload(...)` is a statement; Godot
    // requires it to follow the header directives.
    if is_gdscript {
        for import in module_imports {
            output.push_str(import);
            output.push('\n');
        }
        if !module_imports.is_empty() {
            output.push('\n');
        }
    }

    // Walk segments in order
    for segment in &source_map.segments {
        match segment {
            Segment::Native { span } => {
                // Extract native text from source bytes
                let text = extract_text(source, span.start, span.end);
                // Expand system instantiations (@@SystemName(args)) in native code
                let expanded = expand_system_instantiations(
                    &text,
                    &defined_system_names,
                    &fsm_names,
                    &params_by_name,
                    lang,
                )?;
                output.push_str(&expanded);
            }

            Segment::Pragma { .. } => {
                // Pragmas are consumed by earlier stages — skip them
                // (they don't appear in the output)
            }

            Segment::System { name, .. } => {
                // Look up generated code for this system
                if let Some(code) = system_code.get(name.as_str()) {
                    // Codegen passes handler-body native regions through
                    // verbatim (Oceans Model). If a handler body contains
                    // `@@OtherSystem($(arg))` that sigil form isn't
                    // expanded by codegen — the handler-body path only
                    // sees the surrounding text as a NativeBlock and
                    // doesn't run the system-instantiation expansion.
                    // Re-running the same expansion over the emitted
                    // system code catches those leaks. Idempotent: if
                    // codegen already stripped every `@@Name(...)`, this
                    // pass finds nothing to rewrite.
                    let expanded = expand_system_instantiations(
                        code,
                        &defined_system_names,
                        &fsm_names,
                        &params_by_name,
                        lang,
                    )?;
                    // GDScript multi-system: every system after the first
                    // must be wrapped as an inner class. GDScript files
                    // accept at most one script-level `extends` directive
                    // and one set of script-level `var`/`func` declarations.
                    // The first system stays as-is at script scope; systems
                    // 2..N strip their leading `extends Base` line and wrap
                    // the rest as `class <Name> extends Base:` with every
                    // subsequent line indented one level.
                    if matches!(lang, TargetLanguage::GDScript) {
                        let is_main = main_system.map(|m| m == name.as_str()).unwrap_or(false);
                        // When the main system's `extends Base` line
                        // was hoisted to the top of the file, strip
                        // it from the in-line emission to avoid a
                        // duplicate `extends`.
                        let rewritten = rewrite_gdscript_per_system(
                            name,
                            &expanded,
                            is_main,
                            is_main && main_extends_line.is_some(),
                        );
                        output.push_str(&rewritten);
                    } else if matches!(lang, TargetLanguage::Rust) {
                        // RFC-0033 #19: wrap the generated content in a
                        // private module with OUTER lint-suppression
                        // attributes. Inner attributes (`#![...]` at
                        // file top) work for standalone-module usage
                        // but rustc rejects them when the file is
                        // pulled in via `include!()`, which is the
                        // canonical Cargo build-script consumer
                        // pattern. Outer attributes on a wrapping
                        // `mod` work in every consumer position; the
                        // `pub use ...::*;` re-export keeps the
                        // public API identical to the unwrapped form.
                        let mod_name = format!("_{}_framec", to_snake_case(name));
                        output.push_str("#[allow(dead_code)]\n");
                        output.push_str("#[allow(non_camel_case_types)]\n");
                        output.push_str("#[allow(non_snake_case)]\n");
                        output.push_str("#[allow(unused_variables)]\n");
                        output.push_str("#[allow(unused_mut)]\n");
                        output.push_str("#[allow(unused_imports)]\n");
                        // Specific clippy lints framec patterns
                        // trigger on the canonical fixture corpus. The
                        // set was audited by running
                        // `cargo clippy -- -D warnings` against every
                        // compiled fixture and collecting every
                        // `-D clippy::<X>` reason fired. Future audits
                        // may add new lints — keep them specific (no
                        // blanket `clippy::all`/`pedantic`/`nursery`)
                        // so genuinely new findings surface to users.
                        output.push_str("#[allow(clippy::assign_op_pattern)]\n");
                        output.push_str("#[allow(clippy::clone_on_copy)]\n");
                        output.push_str("#[allow(clippy::derivable_impls)]\n");
                        output.push_str("#[allow(clippy::match_single_binding)]\n");
                        output.push_str("#[allow(clippy::needless_return)]\n");
                        output.push_str("#[allow(clippy::new_without_default)]\n");
                        output.push_str("#[allow(clippy::single_match)]\n");
                        output.push_str(&format!("mod {} {{\n", mod_name));
                        // `use super::*;` so multi-system files can
                        // reference siblings (re-exported at parent
                        // scope by their own `pub use`).
                        output.push_str("    use super::*;\n");
                        // RFC issue #31/#33 (no_std): the runtime references
                        // `alloc::rc::Rc` / `alloc::collections::BTreeMap`
                        // (and `core::any::Any`) by portable path rather than
                        // `std::*`, and uses the `vec!` / `format!` macros.
                        // `extern crate alloc;` links the alloc crate so the
                        // type paths resolve; `use alloc::{vec, format};`
                        // brings the macros into scope so the module is fully
                        // self-contained under `#![no_std]` — the consumer
                        // provides only the heap *types* (`String`/`Vec`/`Box`)
                        // via its include site, no `#[macro_use]` needed (#33).
                        // Both are no-ops in hosted builds (std re-exports
                        // alloc + prelude macros) and scoped to the wrapping
                        // `mod` so they are legal under `include!()`.
                        //
                        // framec's Rust output targets **edition 2018+**: the
                        // `use alloc::...` import is crate-relative and does
                        // not resolve under edition 2015 (bare `rustc foo.rs`
                        // with no `--edition`). Every real consumer compiles in
                        // a Cargo crate (2018/2021/2024) or via `include!` into
                        // one, so edition 2015 is not supported — that path was
                        // only ever hit by tooling invoking `rustc` without an
                        // edition flag (see FRAMEC_BUGS #31/#33).
                        output.push_str("    extern crate alloc;\n");
                        output.push_str("    use alloc::{vec, format};\n");
                        // Indent the generated content by 4 spaces.
                        for line in expanded.lines() {
                            if line.is_empty() {
                                output.push('\n');
                            } else {
                                output.push_str("    ");
                                output.push_str(line);
                                output.push('\n');
                            }
                        }
                        output.push_str("}\n");
                        output.push_str(&format!("pub use {}::*;\n", mod_name));
                    } else {
                        output.push_str(&expanded);
                    }
                } else {
                    return Err(AssemblyError {
                        message: format!(
                            "No generated code for system '{}'. Available: {:?}",
                            name,
                            system_code.keys().collect::<Vec<_>>()
                        ),
                    });
                }
            }

            Segment::Fsm { name, .. } => {
                // RFC-0042: emit the generated recognizer in place of the
                // `@@fsm` block. Generation happened in `do_segment` after
                // the block validated; an absent entry means the target has
                // no @@fsm backend (the pipeline already raised E740), so we
                // emit nothing rather than fail assembly.
                if let Some((_, code)) = generated_fsms.iter().find(|(n, _)| n == name) {
                    output.push_str(code);
                    if !code.ends_with('\n') {
                        output.push('\n');
                    }
                }
            }
        }
    }

    // Erlang attribute hoist. Frame source typically has a native
    // prolog with helper functions BEFORE the `@@system` block:
    //
    //   helper() -> ok.            <-- native, emitted first
    //
    //   @@system Foo { ... }       <-- system code:
    //                                  -module(foo). -behaviour(gen_statem).
    //                                  -export([...]). callbacks() ...
    //
    // Erlang requires `-module`/`-behaviour`/`-export` to precede ANY
    // function definition in the source file — otherwise erlc rejects
    // with "no module definition" + "attribute X after function
    // definitions". Walk the assembled output and pull every leading
    // `-` attribute line up to the top of the file, preserving their
    // relative order. Other lines (comments, helper functions,
    // generated callbacks) keep their original sequence in the
    // remainder.
    if lang == TargetLanguage::Erlang {
        // Multi-line attributes (e.g., `-record(data, { ... }).`) are
        // detected by an opening `-attr(` line whose closing `).` is
        // on a later line — track paren depth across lines and
        // keep the whole block together.
        let lines: Vec<&str> = output.lines().collect();
        let mut attrs: Vec<&str> = Vec::new();
        let mut other: Vec<&str> = Vec::new();
        let mut idx = 0;
        while idx < lines.len() {
            let line = lines[idx];
            let t = line.trim_start();
            let is_attr_start = t.starts_with("-module(")
                || t.starts_with("-behaviour(")
                || t.starts_with("-behavior(")
                || t.starts_with("-export(")
                || t.starts_with("-record(")
                || t.starts_with("-define(")
                || t.starts_with("-include(")
                || t.starts_with("-include_lib(");
            if !is_attr_start {
                other.push(line);
                idx += 1;
                continue;
            }
            // Collect this line and any continuation lines until paren
            // depth reaches zero AND we've seen the terminating `).`.
            let mut depth: i32 = 0;
            let mut closed = false;
            let start = idx;
            while idx < lines.len() {
                let l = lines[idx];
                for c in l.chars() {
                    match c {
                        '(' => depth += 1,
                        ')' => depth -= 1,
                        _ => {}
                    }
                }
                idx += 1;
                if depth <= 0 && l.trim_end().ends_with(").") {
                    closed = true;
                    break;
                }
            }
            for l in &lines[start..idx] {
                attrs.push(l);
            }
            // If the block didn't close cleanly (defensive — shouldn't
            // happen in well-formed Erlang), the bytes end up in
            // `attrs` and we move on.
            let _ = closed;
        }
        if !attrs.is_empty() {
            let mut hoisted = String::new();
            for a in &attrs {
                hoisted.push_str(a);
                hoisted.push('\n');
            }
            hoisted.push('\n');
            for o in &other {
                hoisted.push_str(o);
                hoisted.push('\n');
            }
            output = hoisted;
        }
    }

    // #120: a Lua module must `return` a table for a host file to reach the
    // systems (`local M = require("x"); M.Game`). Each system/FSM is declared
    // `local <Name> = {}` at top level, so they are in scope here. The export
    // goes at the very END — after any user epilog — because Lua requires
    // `return` to be a block's last statement. (A self-contained script with a
    // `main` still runs it first; the trailing `return` is harmless.)
    if matches!(lang, TargetLanguage::Lua) {
        let mut names: Vec<&str> = generated_systems.iter().map(|(n, _)| n.as_str()).collect();
        names.extend(generated_fsms.iter().map(|(n, _)| n.as_str()));
        if !names.is_empty() {
            if !output.ends_with('\n') {
                output.push('\n');
            }
            output.push_str("\nreturn {\n");
            for n in &names {
                output.push_str(&format!("    {n} = {n},\n"));
            }
            output.push_str("}\n");
        }
    }

    // #171: Go has no inline imports and requires the import block after the
    // `package` clause, so the persist codegen's `encoding/json` use can't ride
    // `runtime_imports` (which precede the prolog, as C++'s nlohmann include
    // does). Inject it right after the `package` line — but only when the file
    // doesn't already import it (a duplicate import is a Go compile error).
    if go_needs_json_import && !output.contains("\"encoding/json\"") {
        let mut insert_at = None;
        let mut offset = 0;
        for line in output.split_inclusive('\n') {
            if line.trim_start().starts_with("package ") {
                insert_at = Some(offset + line.len());
                break;
            }
            offset += line.len();
        }
        if let Some(at) = insert_at {
            output.insert_str(at, "\nimport \"encoding/json\"\n");
        }
    }

    Ok(output)
}

// ============================================================================
// Internal: Text Extraction
// ============================================================================

/// Extract text from source bytes at the given byte range.
fn extract_text(source: &[u8], start: usize, end: usize) -> String {
    let end = end.min(source.len());
    let start = start.min(end);
    String::from_utf8_lossy(&source[start..end]).into_owned()
}

/// Did the GDScript codegen emit this system at script-module scope?
///
/// Module-scope emission starts with a leading `extends <Base>` line
/// (after any blank lines) and lays the system's fields/methods flat
/// at indent 0. Systems without a declared base are wrapped by codegen
/// as `class <Name>:` inner classes from the start — those don't need
/// the assembler-level wrap (it would double-nest the class).
fn per_system_emits_at_module_scope(code: &str) -> bool {
    code.lines()
        .map(|l| l.trim_end())
        .find(|l| !l.is_empty())
        .map(|l| l.starts_with("extends "))
        .unwrap_or(false)
}

/// Rewrite a single per-system GDScript emission according to whether
/// it's the file's `@@[main]` system (RFC-0014):
///
/// * **Main system** (`is_main == true`): pass through unchanged.
///   Its per-system codegen owns the script-level `extends` directive
///   and any `var` / `func` declarations. From here, references to
///   non-main systems resolve as `Inner.new()` — sibling inner
///   classes are visible from the script's own `_init` and method
///   bodies.
///
/// * **Non-main systems** whose codegen emitted at script-module
///   scope (leading `extends <Base>`): wrap as `class <name> extends
///   <Base>:` with the body indented one level. Sibling inner classes
///   in GDScript can reference each other by bare name.
///
/// * **Non-main systems** whose codegen already produced inner-class
///   form (`class <name>:`, no declared base): pass through unchanged.
///   Wrapping again would double-nest the class.
///
/// All wrapping/indenting work delegates to the Frame state machine
/// in `compiler/gdscript_multisys/multisys_assembler.frs`.
fn rewrite_gdscript_per_system(
    name: &str,
    code: &str,
    is_main: bool,
    strip_leading_extends: bool,
) -> String {
    use crate::frame_c::compiler::gdscript_multisys;

    if is_main {
        if strip_leading_extends {
            strip_leading_extends_line(code)
        } else {
            code.to_string()
        }
    } else if per_system_emits_at_module_scope(code) {
        gdscript_multisys::wrap_inner(name, code)
    } else {
        code.to_string()
    }
}

/// Find the leading `extends <Base>` line in a per-system GDScript
/// emission and return it (without the trailing newline). Used by the
/// main-system hoist so the script-level `extends` directive lands at
/// the very top of the file, before any inner-class declarations from
/// non-main systems.
///
/// Returns None when the system's emission doesn't begin with an
/// `extends` directive — typically because codegen wrapped it as
/// inner-class form (no declared base in source). In that case the
/// hoist is a no-op.
fn extract_leading_extends_line(code: &str) -> Option<String> {
    for line in code.lines() {
        let t = line.trim_start();
        if t.is_empty() {
            continue;
        }
        if let Some(rest) = t.strip_prefix("extends ") {
            return Some(format!("extends {}", rest.trim_end()));
        }
        return None;
    }
    None
}

/// Drop the first `extends <Base>` line (and any blank lines
/// immediately preceding it) from a per-system emission. Mirrors the
/// inverse of `extract_leading_extends_line`.
fn strip_leading_extends_line(code: &str) -> String {
    let mut lines = code.lines().peekable();
    let mut leading_blanks: Vec<&str> = Vec::new();
    while let Some(&l) = lines.peek() {
        if l.trim().is_empty() {
            leading_blanks.push(l);
            lines.next();
        } else {
            break;
        }
    }
    let stripped = match lines.peek() {
        Some(l) if l.trim_start().starts_with("extends ") => {
            lines.next();
            true
        }
        _ => false,
    };
    if !stripped {
        return code.to_string();
    }
    let mut out = String::with_capacity(code.len());
    for l in leading_blanks {
        out.push_str(l);
        out.push('\n');
    }
    for l in lines {
        out.push_str(l);
        out.push('\n');
    }
    out
}

/// Render a `CallArgsError` as a human-readable assembly diagnostic.
fn format_call_args_error(err: &CallArgsError) -> String {
    match err {
        CallArgsError::ParseError { message, position } => {
            format!("parse error at {}: {}", position, message)
        }
        CallArgsError::MixedForms { message } => message.clone(),
        CallArgsError::SigilsRequired { message } => message.clone(),
        CallArgsError::PositionalMismatch { message } => message.clone(),
        CallArgsError::UnknownNamedArg { name } => {
            format!("unknown named argument '{}'", name)
        }
        CallArgsError::MissingArg { name } => {
            format!(
                "required parameter '{}' has no argument and no default",
                name
            )
        }
        CallArgsError::ExtraArgs { count } => {
            format!("{} extra argument(s) supplied", count)
        }
        CallArgsError::DuplicateNamedArg { name } => {
            format!("duplicate named argument '{}'", name)
        }
    }
}

// ============================================================================
// Internal: System Instantiation Expansion
// ============================================================================

/// Expand `@@SystemName(args)` system instantiations in native code.
///
/// In native code regions, users write `@@SystemName(args)` which gets expanded
/// to the appropriate constructor syntax for the target language:
/// - Python: `SystemName(args)`
/// - TypeScript: `new SystemName(args)`
/// - Rust: `SystemName::new(args)`
/// - C: `SystemName_new(args)`
/// - C++/Java/C#: `new SystemName(args)`
fn expand_system_instantiations(
    text: &str,
    defined_systems: &HashSet<String>,
    fsm_names: &HashSet<&str>,
    params_by_name: &HashMap<&str, &[SystemParam]>,
    lang: TargetLanguage,
) -> Result<String, AssemblyError> {
    // RFC-0035 Round 10: lexing the native region into Literal/Call tokens is
    // a Frame FSM (`compiler/call_site_scanner/`). Expansion stays here — it
    // needs the (borrowed) system-params maps, which never enter the FSM.
    use crate::frame_c::compiler::call_site_scanner::{scan_call_sites, CallToken};
    let mut result = String::new();
    for tok in scan_call_sites(text, lang) {
        match tok {
            CallToken::Literal(s) => result.push_str(&s),
            CallToken::Call {
                name,
                args,
                no_init,
            } => {
                let rendered = expand_one(
                    &name,
                    &args,
                    no_init,
                    defined_systems,
                    fsm_names,
                    params_by_name,
                    lang,
                )?;
                result.push_str(&rendered);
            }
        }
    }
    Ok(result)
}

/// Render one `@@[!]Name(args)` call-site to its target-language constructor.
/// `defined` systems resolve their args against the declared param shape;
/// cross-file systems pass args through verbatim (RFC-0024 — framec does not
/// verify cross-unit name resolution). `@@!Name()` is the no-init form.
fn expand_one(
    name: &str,
    args_text: &str,
    is_no_init: bool,
    defined_systems: &HashSet<String>,
    fsm_names: &HashSet<&str>,
    params_by_name: &HashMap<&str, &[SystemParam]>,
    lang: TargetLanguage,
) -> Result<String, AssemblyError> {
    if is_no_init {
        return Ok(
            crate::frame_c::compiler::codegen::frame_expansion::generate_no_initialization(
                name, lang,
            ),
        );
    }
    // RFC-0042 Mode A/B: `@@FsmName(args)` constructs an fsm instance. Unlike
    // a `@@system` (RFC-0017 `._create` factory), an `@@fsm` runs recognition
    // in its plain constructor, so the call site is just `FsmName(args)`.
    if fsm_names.contains(name) {
        return Ok(generate_fsm_constructor(name, args_text, lang));
    }
    if defined_systems.contains(name) {
        let resolved_args = match params_by_name.get(name) {
            Some(params) if !params.is_empty() => {
                let parsed = parse_call_args(args_text).map_err(|e| AssemblyError {
                    message: format!("@@{}({}): {}", name, args_text, format_call_args_error(&e)),
                })?;
                let values = resolve_call(&parsed, params).map_err(|e| AssemblyError {
                    message: format!("@@{}({}): {}", name, args_text, format_call_args_error(&e)),
                })?;
                values.join(", ")
            }
            _ => args_text.to_string(),
        };
        Ok(generate_constructor(name, &resolved_args, lang))
    } else {
        // Cross-file system: no params metadata; pass args through verbatim.
        Ok(generate_constructor(name, args_text, lang))
    }
}

/// Generate the constructor call for an `@@fsm` (RFC-0042 Mode A/B). An
/// fsm runs recognition in its plain constructor — no RFC-0017 factory —
/// so this is `Name(args)` in Python (v0.1's only fsm backend); other
/// targets fall back to the same plain spelling (they can't reach here in
/// v0.1 — a non-Python `@@fsm` is blocked by E740 before assembly).
fn generate_fsm_constructor(name: &str, args: &str, lang: TargetLanguage) -> String {
    match lang {
        TargetLanguage::TypeScript
        | TargetLanguage::JavaScript
        | TargetLanguage::Cpp
        | TargetLanguage::Java
        | TargetLanguage::CSharp => format!("new {}({})", name, args),
        _ => format!("{}({})", name, args),
    }
}

/// Generate the language-appropriate constructor call for a system.
///
/// Exposed `pub(crate)` so codegen-side expanders (notably
/// `system_codegen::expand_system_instantiation_in_domain` for
/// `@@SystemName(args)` in domain-field initializers) emit the same
/// RFC-0017 factory spelling as the assembler text-rewrite pass.
pub(crate) fn generate_constructor(name: &str, args: &str, lang: TargetLanguage) -> String {
    match lang {
        TargetLanguage::Python3 => {
            // RFC-0017 Phase A0: factory call uses `_create` classmethod
            // which does the two-step (bare ctor + `_frame_init`).
            // Single-underscore avoids Python name-mangling.
            // Bare `Counter()` is reserved for `@@!Counter()` (no init).
            format!("{}._create({})", name, args)
        }
        TargetLanguage::TypeScript | TargetLanguage::JavaScript => {
            // RFC-0017 Phase A5: factory expansion uses `Counter._create(args)`.
            // Bare `new Counter()` reserved for `@@!Counter()`.
            format!("{}._create({})", name, args)
        }
        TargetLanguage::Rust => {
            // RFC-0017 Phase A1: factory call uses `__create` which does
            // the two-step (bare `new()` + `__frame_init(args)`).
            // Bare `Counter::new()` is reserved for `@@!Counter()`.
            if args.trim().is_empty() {
                format!("{}::__create()", name)
            } else {
                format!("{}::__create({})", name, args)
            }
        }
        TargetLanguage::C => {
            // RFC-0017 Phase A3: C factory expansion uses `Foo_create()`.
            // Bare `Foo_new()` is reserved for `@@!Foo()` (framework only).
            if args.trim().is_empty() {
                format!("{}_create()", name)
            } else {
                format!("{}_create({})", name, args)
            }
        }
        TargetLanguage::Java => {
            // RFC-0017 Phase A1: Java factory expansion uses `__create()`.
            // Bare `new Counter()` is reserved for `@@!Counter()`.
            format!("{}.__create({})", name, args)
        }
        TargetLanguage::CSharp => {
            // RFC-0017 Phase A2: C# factory expansion uses `__create()`.
            // Bare `new Counter()` is reserved for `@@!Counter()`.
            format!("{}.__create({})", name, args)
        }
        TargetLanguage::Php => {
            // RFC-0017 Phase A5: PHP factory expansion uses
            // `Counter::_create(args)`. Bare `new Counter()` is
            // reserved for `@@!Counter()`.
            format!("{}::_create({})", name, args)
        }
        TargetLanguage::Kotlin => {
            // RFC-0017 Phase A1: Kotlin factory expansion uses `__create()`
            // companion-object method. Bare `Counter()` is reserved for
            // `@@!Counter()` (no init).
            format!("{}.__create({})", name, args)
        }
        TargetLanguage::Cpp => {
            // RFC-0017 Phase A3: C++ factory expansion uses `Counter::__create(args)`.
            // Bare `Counter()` is reserved for `@@!Counter()` (framework only).
            format!("{}::__create({})", name, args)
        }
        TargetLanguage::Go => {
            // RFC-0017 Phase A2: Go factory expansion uses `CreateName()`.
            // Bare `NewName()` is reserved for `@@!Name()` (framework only).
            if args.trim().is_empty() {
                format!("Create{}()", name)
            } else {
                format!("Create{}({})", name, args)
            }
        }
        TargetLanguage::Ruby => {
            // RFC-0017 Phase A5: Ruby factory expansion uses
            // `Counter._create(args)`. Bare `Counter.new` reserved for
            // `@@!Counter()`.
            if args.trim().is_empty() {
                format!("{}._create", name)
            } else {
                format!("{}._create({})", name, args)
            }
        }
        TargetLanguage::Swift => {
            // RFC-0017 Phase A2: Swift factory expansion uses `__create()`.
            // Bare `Counter()` is reserved for `@@!Counter()`.
            format!("{}.__create({})", name, args)
        }
        TargetLanguage::Erlang => {
            // RFC-0017 Phase A6: Erlang factory expansion uses
            // `module:create(args)` which returns a bare Pid (no
            // `element(2, ...)` unwrap needed). Bare `module:start_link()`
            // is reserved for `@@!Counter()`.
            let module_name = {
                let mut result = String::new();
                for (i, c) in name.chars().enumerate() {
                    if c.is_uppercase() && i > 0 {
                        result.push('_');
                    }
                    if let Some(lc) = c.to_lowercase().next() {
                        result.push(lc);
                    }
                }
                result
            };
            if args.trim().is_empty() {
                format!("{}:create()", module_name)
            } else {
                format!("{}:create({})", module_name, args)
            }
        }
        TargetLanguage::Lua => {
            // RFC-0017 Phase A5: Lua factory expansion uses
            // `Counter._create(args)`. Bare `Counter.new()` is
            // reserved for `@@!Counter()`.
            if args.trim().is_empty() {
                format!("{}._create()", name)
            } else {
                format!("{}._create({})", name, args)
            }
        }
        TargetLanguage::Dart => {
            // RFC-0017 Phase A4: Dart factory expansion uses the public
            // `Counter.create(args)` factory constructor (#108 — `_create`
            // was library-private). Bare `Counter()` is reserved for `@@!Counter()`.
            format!("{}.create({})", name, args)
        }
        TargetLanguage::GDScript => {
            // RFC-0017 Phase A4: GDScript factory expansion uses
            // `ClassName._create(args)`. Bare `ClassName.new()` is
            // reserved for `@@!ClassName()`.
            if args.trim().is_empty() {
                format!("{}._create()", name)
            } else {
                format!("{}._create({})", name, args)
            }
        }
        // Non-V4 targets should never reach the assembler.
        // No _ => arm: compiler enforces new TargetLanguage variants are added here.
        TargetLanguage::Graphviz => {
            unreachable!("Assembler called for non-V4 target {:?}", lang)
        }
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frame_c::compiler::segmenter::Span;

    /// Helper: make a SourceMap manually for testing
    fn make_source_map(source: &str, segments: Vec<Segment>) -> SourceMap {
        SourceMap {
            segments,
            source: source.as_bytes().to_vec(),
            target: Some(TargetLanguage::Python3),
        }
    }

    #[test]
    fn test_native_only() {
        let src = "import math\nprint('hello')\n";
        let map = make_source_map(
            src,
            vec![Segment::Native {
                span: Span {
                    start: 0,
                    end: src.len(),
                },
            }],
        );
        let result = assemble(
            &map,
            &[],
            &[],
            &[],
            TargetLanguage::Python3,
            &[],
            &[],
            &[],
            None,
            false,
        )
        .unwrap();
        assert_eq!(result, src);
    }

    #[test]
    fn test_system_replacement() {
        let src = "prolog\n@@system Foo {\n  machine:\n    $A { }\n}\nepilogue\n";
        let prolog_end = 7;
        let system_start = 7;
        let system_end = 46;
        let epilog_start = 46;
        let map = make_source_map(
            src,
            vec![
                Segment::Native {
                    span: Span {
                        start: 0,
                        end: prolog_end,
                    },
                },
                Segment::System {
                    outer_span: Span {
                        start: system_start,
                        end: system_end,
                    },
                    body_span: Span {
                        start: system_start + 16,
                        end: system_end - 1,
                    },
                    header_params_span: None,
                    name: "Foo".to_string(),
                    bases: vec![],
                    visibility: None,
                },
                Segment::Native {
                    span: Span {
                        start: epilog_start,
                        end: src.len(),
                    },
                },
            ],
        );
        let generated = vec![("Foo".to_string(), "class Foo:\n  pass\n".to_string())];
        let result = assemble(
            &map,
            &generated,
            &[],
            &[],
            TargetLanguage::Python3,
            &[],
            &[],
            &[],
            None,
            false,
        )
        .unwrap();
        assert_eq!(result, "prolog\nclass Foo:\n  pass\nepilogue\n");
    }

    #[test]
    fn test_pragma_skipped() {
        let src = "@@target python_3\nimport os\n";
        let map = make_source_map(
            src,
            vec![
                Segment::Pragma {
                    kind: crate::frame_c::compiler::segmenter::PragmaKind::Target,
                    span: Span { start: 0, end: 18 },
                    value: Some("python_3".to_string()),
                    is_broadcast: false,
                },
                Segment::Native {
                    span: Span {
                        start: 18,
                        end: src.len(),
                    },
                },
            ],
        );
        let result = assemble(
            &map,
            &[],
            &[],
            &[],
            TargetLanguage::Python3,
            &[],
            &[],
            &[],
            None,
            false,
        )
        .unwrap();
        assert_eq!(result, "import os\n");
    }

    /// Helper that builds an empty params lookup. Most of these tests
    /// exercise zero-arg or raw passthrough flows where the system has no
    /// declared params and the assembler is expected to forward the
    /// args text unchanged.
    fn empty_params() -> HashMap<&'static str, &'static [SystemParam]> {
        HashMap::new()
    }

    #[test]
    fn test_system_instantiation_python() {
        // RFC-0017 Phase A0: Python factory expansion uses `_create`
        // classmethod (init-decoupled). Bare `Foo()` is reserved for
        // `@@!Foo()` (no-init).
        let src = "s = @@Foo()\n";
        let systems: HashSet<String> = vec!["Foo".to_string()].into_iter().collect();
        let params = empty_params();
        let result = expand_system_instantiations(
            src,
            &systems,
            &std::collections::HashSet::<&str>::new(),
            &params,
            TargetLanguage::Python3,
        )
        .unwrap();
        assert_eq!(result, "s = Foo._create()\n");
    }

    #[test]
    fn test_system_instantiation_typescript() {
        // RFC-0017 Phase A5: TS factory expansion uses `Foo._create()`.
        let src = "let s = @@Foo()\n";
        let systems: HashSet<String> = vec!["Foo".to_string()].into_iter().collect();
        let params = empty_params();
        let result = expand_system_instantiations(
            src,
            &systems,
            &std::collections::HashSet::<&str>::new(),
            &params,
            TargetLanguage::TypeScript,
        )
        .unwrap();
        assert_eq!(result, "let s = Foo._create()\n");
    }

    #[test]
    fn test_system_instantiation_rust() {
        // RFC-0017 Phase A1: Rust factory expansion uses `__create()`.
        // Bare `Foo::new()` is reserved for `@@!Foo()`.
        let src = "let s = @@Foo();\n";
        let systems: HashSet<String> = vec!["Foo".to_string()].into_iter().collect();
        let params = empty_params();
        let result = expand_system_instantiations(
            src,
            &systems,
            &std::collections::HashSet::<&str>::new(),
            &params,
            TargetLanguage::Rust,
        )
        .unwrap();
        assert_eq!(result, "let s = Foo::__create();\n");
    }

    #[test]
    fn test_system_instantiation_c() {
        // RFC-0017 Phase A3: C factory expansion uses `Foo_create()`.
        // Bare `Foo_new()` is reserved for `@@!Foo()`.
        let src = "struct Foo* s = @@Foo();\n";
        let systems: HashSet<String> = vec!["Foo".to_string()].into_iter().collect();
        let params = empty_params();
        let result = expand_system_instantiations(
            src,
            &systems,
            &std::collections::HashSet::<&str>::new(),
            &params,
            TargetLanguage::C,
        )
        .unwrap();
        assert_eq!(result, "struct Foo* s = Foo_create();\n");
    }

    #[test]
    fn test_system_instantiation_with_args() {
        // RFC-0017 Phase A0: Python factory expansion uses `_create`.
        let src = "s = @@Foo(1, \"hello\")\n";
        let systems: HashSet<String> = vec!["Foo".to_string()].into_iter().collect();
        let params = empty_params();
        let result = expand_system_instantiations(
            src,
            &systems,
            &std::collections::HashSet::<&str>::new(),
            &params,
            TargetLanguage::Python3,
        )
        .unwrap();
        assert_eq!(result, "s = Foo._create(1, \"hello\")\n");
    }

    #[test]
    fn test_cross_file_system_lowers_without_validation() {
        // RFC-0024 / issue #29: `@@SystemName(args)` referencing a
        // system NOT declared in this compile unit must lower to the
        // target's factory call using just the literal name. Host
        // language resolves the name at host-compile time.
        let src = "let x = @@Bar(7);\n";
        let systems: HashSet<String> = vec!["Foo".to_string()].into_iter().collect();
        let params = empty_params();
        let result = expand_system_instantiations(
            src,
            &systems,
            &std::collections::HashSet::<&str>::new(),
            &params,
            TargetLanguage::Rust,
        )
        .unwrap();
        assert_eq!(result, "let x = Bar::__create(7);\n");
    }

    #[test]
    fn test_cross_file_system_no_init_form() {
        // RFC-0024 / issue #29: `@@!SystemName()` no-init form for a
        // cross-file system must also lower without validation.
        let src = "let x = @@!Bar();\n";
        let systems: HashSet<String> = vec!["Foo".to_string()].into_iter().collect();
        let params = empty_params();
        let result = expand_system_instantiations(
            src,
            &systems,
            &std::collections::HashSet::<&str>::new(),
            &params,
            TargetLanguage::Rust,
        )
        .unwrap();
        assert_eq!(result, "let x = Bar::new();\n");
    }

    #[test]
    fn test_cross_file_system_args_passthrough() {
        // For unknown (cross-file) systems framec has no param
        // metadata for default-arg expansion. The user's args text
        // passes through verbatim — matching the cross-file system's
        // signature is the user's responsibility, the same contract
        // host language compilers use for any cross-module call.
        let src = "s = @@Bar(a, b, c)\n";
        let systems: HashSet<String> = vec!["Foo".to_string()].into_iter().collect();
        let params = empty_params();
        let result = expand_system_instantiations(
            src,
            &systems,
            &std::collections::HashSet::<&str>::new(),
            &params,
            TargetLanguage::Python3,
        )
        .unwrap();
        assert_eq!(result, "s = Bar._create(a, b, c)\n");
    }

    #[test]
    fn test_system_instantiation_in_comment_not_expanded() {
        let src = "# s = @@Foo()\n";
        let systems: HashSet<String> = vec!["Foo".to_string()].into_iter().collect();
        let params = empty_params();
        let result = expand_system_instantiations(
            src,
            &systems,
            &std::collections::HashSet::<&str>::new(),
            &params,
            TargetLanguage::Python3,
        )
        .unwrap();
        assert_eq!(result, "# s = @@Foo()\n");
    }

    #[test]
    fn test_system_instantiation_in_string_not_expanded() {
        let src = "s = \"@@Foo()\"\n";
        let systems: HashSet<String> = vec!["Foo".to_string()].into_iter().collect();
        let params = empty_params();
        let result = expand_system_instantiations(
            src,
            &systems,
            &std::collections::HashSet::<&str>::new(),
            &params,
            TargetLanguage::Python3,
        )
        .unwrap();
        assert_eq!(result, "s = \"@@Foo()\"\n");
    }

    #[test]
    fn test_multiple_systems() {
        // source with prolog, two systems, interstitial native, epilog
        let src = "prolog\n__SYS1__\nnative_between\n__SYS2__\nepilogue\n";
        let s1_start = 7;
        let s1_end = 16;
        let between_start = 16;
        let between_end = 31;
        let s2_start = 31;
        let s2_end = 40;
        let epilog_start = 40;

        let map = make_source_map(
            src,
            vec![
                Segment::Native {
                    span: Span {
                        start: 0,
                        end: s1_start,
                    },
                },
                Segment::System {
                    outer_span: Span {
                        start: s1_start,
                        end: s1_end,
                    },
                    body_span: Span {
                        start: s1_start + 2,
                        end: s1_end - 2,
                    },
                    header_params_span: None,
                    name: "Alpha".to_string(),
                    bases: vec![],
                    visibility: None,
                },
                Segment::Native {
                    span: Span {
                        start: between_start,
                        end: between_end,
                    },
                },
                Segment::System {
                    outer_span: Span {
                        start: s2_start,
                        end: s2_end,
                    },
                    body_span: Span {
                        start: s2_start + 2,
                        end: s2_end - 2,
                    },
                    header_params_span: None,
                    name: "Beta".to_string(),
                    bases: vec![],
                    visibility: None,
                },
                Segment::Native {
                    span: Span {
                        start: epilog_start,
                        end: src.len(),
                    },
                },
            ],
        );

        let generated = vec![
            ("Alpha".to_string(), "class Alpha: pass\n".to_string()),
            ("Beta".to_string(), "class Beta: pass\n".to_string()),
        ];
        let result = assemble(
            &map,
            &generated,
            &[],
            &[],
            TargetLanguage::Python3,
            &[],
            &[],
            &[],
            None,
            false,
        )
        .unwrap();
        assert!(result.contains("prolog\n"));
        assert!(result.contains("class Alpha: pass\n"));
        assert!(result.contains("\nnative_between\n"));
        assert!(result.contains("class Beta: pass\n"));
        assert!(result.contains("epilogue\n"));
    }

    #[test]
    fn test_missing_system_code_errors() {
        let src = "@@system Foo { }";
        let map = make_source_map(
            src,
            vec![Segment::System {
                outer_span: Span {
                    start: 0,
                    end: src.len(),
                },
                body_span: Span { start: 14, end: 15 },
                header_params_span: None,
                name: "Foo".to_string(),
                bases: vec![],
                visibility: None,
            }],
        );
        let result = assemble(
            &map,
            &[],
            &[],
            &[],
            TargetLanguage::Python3,
            &[],
            &[],
            &[],
            None,
            false,
        );
        assert!(result.is_err());
        assert!(result.unwrap_err().message.contains("Foo"));
    }

    #[test]
    fn test_undefined_system_instantiation_passes_through() {
        // RFC-0024 / issue #29: `@@SystemName(args)` for a name NOT
        // declared in this compile unit must lower verbatim to the
        // target's factory call. framec MUST NOT verify that the
        // name corresponds to a declaration anywhere — host language
        // resolves it at host-compile time. This test originally
        // asserted the pre-RFC-0024 reject behavior; inverted here
        // to match the spec.
        let src = "s = @@Unknown()\n";
        let systems: HashSet<String> = HashSet::new();
        let params: HashMap<&str, &[SystemParam]> = HashMap::new();
        let result = expand_system_instantiations(
            src,
            &systems,
            &std::collections::HashSet::<&str>::new(),
            &params,
            TargetLanguage::Python3,
        )
        .unwrap();
        assert_eq!(result, "s = Unknown._create()\n");
    }

    #[test]
    fn test_full_assembly_with_system_instantiation() {
        // prolog creates an instance using system instantiation
        let src_native = "s = @@MySystem()\n";
        let src_system = "@@system MySystem { machine: $A { } }";
        let full_src = format!("{}{}", src_native, src_system);
        let native_end = src_native.len();

        let map = make_source_map(
            &full_src,
            vec![
                Segment::Native {
                    span: Span {
                        start: 0,
                        end: native_end,
                    },
                },
                Segment::System {
                    outer_span: Span {
                        start: native_end,
                        end: full_src.len(),
                    },
                    body_span: Span {
                        start: native_end + 20,
                        end: full_src.len() - 1,
                    },
                    header_params_span: None,
                    name: "MySystem".to_string(),
                    bases: vec![],
                    visibility: None,
                },
            ],
        );

        let generated = vec![(
            "MySystem".to_string(),
            "class MySystem:\n  pass\n".to_string(),
        )];
        let result = assemble(
            &map,
            &generated,
            &[],
            &[],
            TargetLanguage::Python3,
            &[],
            &[],
            &[],
            None,
            false,
        )
        .unwrap();
        // RFC-0017 Phase A0: Python factory expansion uses `_create`.
        assert_eq!(result, "s = MySystem._create()\nclass MySystem:\n  pass\n");
    }

    #[test]
    fn test_runtime_imports_before_prolog() {
        // Test that runtime imports are emitted before native prolog code
        let src = "import json\n@@system Foo { machine: $A { } }";
        let prolog_end = 12;
        let map = make_source_map(
            src,
            vec![
                Segment::Native {
                    span: Span {
                        start: 0,
                        end: prolog_end,
                    },
                },
                Segment::System {
                    outer_span: Span {
                        start: prolog_end,
                        end: src.len(),
                    },
                    body_span: Span {
                        start: prolog_end + 16,
                        end: src.len() - 1,
                    },
                    header_params_span: None,
                    name: "Foo".to_string(),
                    bases: vec![],
                    visibility: None,
                },
            ],
        );
        let generated = vec![("Foo".to_string(), "class Foo:\n  pass\n".to_string())];
        let runtime_imports = vec!["from typing import Any".to_string()];
        let result = assemble(
            &map,
            &generated,
            &[],
            &[],
            TargetLanguage::Python3,
            &runtime_imports,
            &[],
            &[],
            None,
            false,
        )
        .unwrap();
        // Runtime imports should come first, then the native prolog, then system
        assert!(result.starts_with("from typing import Any\n"));
        assert!(result.contains("\nimport json\n"));
        assert!(result.contains("class Foo:"));
    }
}
