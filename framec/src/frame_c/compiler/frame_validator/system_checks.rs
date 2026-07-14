//! System-level validation: self-call routing, system-instantiation arity,
//! and the scanner-based walk that authenticates Frame `@@` segments inside
//! handler/action bodies.
//!
//! These methods catch:
//! - E416/E417/E418 — `@@:self(...)` shape and receiver
//! - E412/E413 — cross-system instantiation references and arity
//! - E419/E420 — `@@:return(...)` / `@@:(value)` placement and arity inside
//!   handler and action bodies (with terminal-statement awareness)

use super::{FrameValidator, ValidationError};
use crate::frame_c::compiler::codegen::frame_expansion::get_native_scanner;
use crate::frame_c::compiler::frame_ast::*;
use crate::frame_c::compiler::native_region_scanner::{
    FrameSegmentKind, Region, RegionSpan, SegmentMetadata,
};
use std::collections::{HashMap, HashSet};

impl FrameValidator {
    pub fn validate_self_calls(
        &mut self,
        ast: &FrameAst,
        source: &[u8],
        target: crate::frame_c::visitors::TargetLanguage,
    ) -> Result<(), Vec<ValidationError>> {
        match ast {
            FrameAst::System(system) => self.validate_system_self_calls(system, source, target),
            FrameAst::Module(module) => {
                for system in &module.systems {
                    self.validate_system_self_calls(system, source, target);
                }
            }
        }

        if self.errors.is_empty() {
            Ok(())
        } else {
            Err(self.errors.clone())
        }
    }

    pub(super) fn validate_system_self_calls(
        &mut self,
        system: &SystemAst,
        source: &[u8],
        target: crate::frame_c::visitors::TargetLanguage,
    ) {
        let interface_methods = self.build_interface_map(system);

        // RFC-0046: `@@:self.<field>` must resolve to a declared domain field
        // (or an interface method). Collect the domain field names so the
        // segment walker can reject unknown members with E609.
        // RFC-0056 P9 (#209): the input-source header param is auto-promoted to a
        // domain field, exactly as `@@fsm` promotes its parameters. It is a real
        // field on the emitted type (`pub text: I`) — it is simply *borrowed*
        // rather than owned, so `@@:self.text.get(i)` must resolve.
        let domain_fields: std::collections::HashSet<String> = system
            .domain
            .iter()
            .map(|d| d.name.clone())
            .chain(
                system
                    .params
                    .iter()
                    .filter(|p| p.kind == crate::frame_c::compiler::frame_ast::ParamKind::Input)
                    .map(|p| p.name.clone()),
            )
            .collect();
        // E617 (#159 round 3): field name → declared type string, for the
        // indexed cross-system call resolvability check on Lua.
        let domain_field_types: std::collections::HashMap<String, String> = system
            .domain
            .iter()
            .filter_map(|d| match &d.var_type {
                crate::frame_c::compiler::frame_ast::Type::Custom(t) => {
                    Some((d.name.clone(), t.clone()))
                }
                crate::frame_c::compiler::frame_ast::Type::Unknown => None,
            })
            .collect();
        let calling_system = system.name.clone();

        // RFC-0046: `@@:self.<action>(args)` is a valid direct action call.
        // Collect action names so the kind-10 walk accepts them instead of
        // rejecting with E601.
        let actions: std::collections::HashSet<String> =
            system.actions.iter().map(|a| a.name.clone()).collect();
        let operations: std::collections::HashSet<String> =
            system.operations.iter().map(|o| o.name.clone()).collect();

        // Validate handler bodies using the scanner (handles comments, strings correctly)
        if let Some(machine) = &system.machine {
            for state in &machine.states {
                for handler in &state.handlers {
                    let span = &handler.span;
                    if span.start >= source.len() || span.end > source.len() {
                        continue;
                    }
                    let body = &source[span.start..span.end];
                    self.validate_frame_segments_in_body(
                        body,
                        &interface_methods,
                        &domain_fields,
                        &domain_field_types,
                        &calling_system,
                        &actions,
                        &operations,
                        &state.name,
                        &handler.event,
                        target,
                    );
                }
            }
        }

        // Also validate action bodies
        for action in &system.actions {
            let span = &action.span;
            if span.start >= source.len() || span.end > source.len() {
                continue;
            }
            let body = &source[span.start..span.end];
            self.validate_frame_segments_in_body(
                body,
                &interface_methods,
                &domain_fields,
                &domain_field_types,
                &calling_system,
                &actions,
                &operations,
                "(action)",
                &action.name,
                target,
            );
        }
    }

    /// RFC-0015 D7: validate `@@SystemName(args)` and `@@!SystemName()` call
    /// sites against the kind-specific rules.
    ///
    /// - **E820**: `@@!Foo(args)` with non-empty args is rejected. The
    ///   no-initialization form is zero-arg by definition. (Same-file rule;
    ///   the validator owns this.)
    /// - **E821 (REMOVED per RFC-0024 / bug #30)**: framec MUST NOT verify
    ///   that `@@SystemName(...)` references a system declared in the
    ///   module. The host language's name resolution reports any miss at
    ///   host-compile time. Bug #29 fixed the assembler path; bug #30
    ///   removed the matching check in this validator.
    pub fn validate_system_instantiations(
        &mut self,
        ast: &FrameAst,
        source: &[u8],
        target: crate::frame_c::visitors::TargetLanguage,
    ) -> Result<(), Vec<ValidationError>> {
        let defined_systems: std::collections::HashSet<String> = match ast {
            FrameAst::System(s) => std::iter::once(s.name.clone()).collect(),
            FrameAst::Module(m) => m.systems.iter().map(|s| s.name.clone()).collect(),
        };

        match ast {
            FrameAst::System(system) => {
                self.validate_system_instantiations_in_system(
                    system,
                    source,
                    target,
                    &defined_systems,
                );
            }
            FrameAst::Module(module) => {
                for system in &module.systems {
                    self.validate_system_instantiations_in_system(
                        system,
                        source,
                        target,
                        &defined_systems,
                    );
                }
            }
        }

        if self.errors.is_empty() {
            Ok(())
        } else {
            Err(self.errors.clone())
        }
    }

    pub(super) fn validate_system_instantiations_in_system(
        &mut self,
        system: &SystemAst,
        source: &[u8],
        target: crate::frame_c::visitors::TargetLanguage,
        defined_systems: &std::collections::HashSet<String>,
    ) {
        if let Some(machine) = &system.machine {
            for state in &machine.states {
                for handler in &state.handlers {
                    let span = &handler.span;
                    if span.start >= source.len() || span.end > source.len() {
                        continue;
                    }
                    let body = &source[span.start..span.end];
                    self.validate_system_instantiations_in_body(body, target, defined_systems);
                }
            }
        }
        for action in &system.actions {
            let span = &action.span;
            if span.start >= source.len() || span.end > source.len() {
                continue;
            }
            let body = &source[span.start..span.end];
            self.validate_system_instantiations_in_body(body, target, defined_systems);
        }
    }

    pub(super) fn validate_system_instantiations_in_body(
        &mut self,
        body: &[u8],
        target: crate::frame_c::visitors::TargetLanguage,
        defined_systems: &std::collections::HashSet<String>,
    ) {
        use crate::frame_c::compiler::frame_ast::InstantiationKind;
        use crate::frame_c::compiler::native_region_scanner::{
            FrameSegmentKind, Region, SegmentMetadata,
        };

        let open_brace = match body.iter().position(|&b| b == b'{') {
            Some(pos) => pos,
            None => return,
        };
        let mut scanner = get_native_scanner(target);
        let scan_result = match scanner.scan(body, open_brace) {
            Ok(r) => r,
            Err(_) => return,
        };

        for region in &scan_result.regions {
            if let Region::FrameSegment {
                kind: FrameSegmentKind::SystemInstantiation,
                metadata:
                    SegmentMetadata::SystemInstantiation {
                        system_name,
                        args,
                        kind: inst_kind,
                    },
                ..
            } = region
            {
                // E820: no-initialization allocation must be zero-arg.
                if *inst_kind == InstantiationKind::NoInitialization {
                    let inner = args.trim_start_matches('(').trim_end_matches(')').trim();
                    if !inner.is_empty() {
                        self.errors.push(ValidationError::new(
                            "E820",
                            format!(
                                "no-initialization allocation `@@!{}({})` must be zero-arg; received: `{}`",
                                system_name, inner, inner
                            ),
                        ));
                    }
                }

                // E821 removed per RFC-0024 — bug #30. framec MUST NOT
                // verify that `@@SystemName(...)` resolves to a declared
                // system. Host language reports any miss at host-compile
                // time. `defined_systems` retained as a parameter so the
                // E820 branch keeps its existing call shape.
                let _ = (system_name, defined_systems);
            }
        }
    }

    /// Re-scan the inner text of return-expression segments to catch a
    /// reserved `@@:system` (E604) / `@@:system.state` (E608) form nested
    /// inside `@@:(…)`, `@@:return(…)`, or `@@:return = …`.
    ///
    /// The top-level segment walk only sees the return segment, not the
    /// reserved form *inside* it, so without this pass those forms slip
    /// past the validator and codegen falls back to emitting a
    /// `/* ERROR: bare @@:system */` placeholder into the output
    /// (RFC-0045 expression-context gap). Reuses the scanner rather than
    /// re-deriving the `@@:system` classification, and recurses so
    /// arbitrarily-nested return expressions (`@@:(@@:(…))`) are covered.
    fn check_reserved_system_in_expr(
        &mut self,
        regions: &[Region],
        scope_outer: &str,
        scope_inner: &str,
        target: crate::frame_c::visitors::TargetLanguage,
    ) {
        for region in regions {
            if let Region::FrameSegment { kind, metadata, .. } = region {
                let inner = match (kind, metadata) {
                    (FrameSegmentKind::ContextReturnExpr, SegmentMetadata::ReturnExpr { expr }) => {
                        Some(expr.as_str())
                    }
                    (FrameSegmentKind::ReturnCall, SegmentMetadata::ReturnCall { expr }) => {
                        Some(expr.as_str())
                    }
                    (
                        FrameSegmentKind::ContextReturn,
                        SegmentMetadata::ContextReturn {
                            assign_expr: Some(expr),
                        },
                    ) => Some(expr.as_str()),
                    _ => None,
                };
                let Some(inner) = inner else { continue };

                // Wrap in a synthetic body so the scanner classifies the
                // inner text exactly as it would in statement position.
                let synthetic = format!("{{{}}}", inner);
                let mut scanner = get_native_scanner(target);
                let scan = match scanner.scan(synthetic.as_bytes(), 0) {
                    Ok(s) => s,
                    Err(_) => continue,
                };
                for r in &scan.regions {
                    if let Region::FrameSegment { kind: k, .. } = r {
                        match k {
                            FrameSegmentKind::ContextSystemBare => {
                                self.errors.push(ValidationError::new(
                                    "E604",
                                    format!(
                                        "bare `@@:system` in {}/{} — `@@:system` requires a member access (e.g. `@@:system.state.name`)",
                                        scope_outer, scope_inner
                                    ),
                                ));
                            }
                            FrameSegmentKind::ContextSystemStateReserved => {
                                self.errors.push(ValidationError::new(
                                    "E608",
                                    format!(
                                        "`@@:system.state` in {}/{} is reserved for future use; use `@@:system.state.name` to read the current state name",
                                        scope_outer, scope_inner
                                    ),
                                ));
                            }
                            _ => {}
                        }
                    }
                }
                // Recurse for nested return expressions.
                self.check_reserved_system_in_expr(&scan.regions, scope_outer, scope_inner, target);
            }
        }
    }

    /// Validate Frame segments in a handler/action body using the scanner.
    /// Runs the language-specific scanner on the body text, then walks the
    /// identified segments. No byte-level scanning — the scanner handles
    /// comments, strings, and language-specific syntax.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn validate_frame_segments_in_body(
        &mut self,
        body: &[u8],
        interface_methods: &HashMap<String, &InterfaceMethod>,
        domain_fields: &std::collections::HashSet<String>,
        domain_field_types: &std::collections::HashMap<String, String>,
        calling_system: &str,
        actions: &std::collections::HashSet<String>,
        operations: &std::collections::HashSet<String>,
        scope_outer: &str,
        scope_inner: &str,
        target: crate::frame_c::visitors::TargetLanguage,
    ) {
        // Find the opening brace
        let open_brace = match body.iter().position(|&b| b == b'{') {
            Some(pos) => pos,
            None => return,
        };

        // Run the scanner
        let mut scanner = get_native_scanner(target);
        let scan_result = match scanner.scan(body, open_brace) {
            Ok(r) => r,
            Err(_) => return, // Scanner error — can't validate
        };

        // Walk segments and validate
        for region in &scan_result.regions {
            if let Region::FrameSegment {
                kind,
                metadata,
                span,
                ..
            } = region
            {
                match kind {
                    // W416 (#130): a bare `@@:return` (read mode — no `= e`,
                    // no `(e)`, no `@@:(e)`) used as a STANDALONE STATEMENT
                    // lowers to a no-op read of the context return slot and
                    // does NOT short-circuit — trailing code still runs. The
                    // getter is semantically valid, but reading-and-discarding
                    // it in statement position has no effect and almost always
                    // means the user wanted `@@:return(e)`, `@@:return = e`,
                    // or native `return`. WARN (do not change semantics).
                    //
                    // "Standalone statement" = the getter is alone on its
                    // logical statement: the bytes from the previous statement
                    // delimiter up to the segment, and from the segment to the
                    // next delimiter, are whitespace only. This excludes the
                    // false-positive case `x = @@:return + 1` (the read is
                    // consumed by a surrounding expression) without judging the
                    // expression's contents. The setter shapes never reach here
                    // (`= e` is `assign_expr: Some`, `(e)`/`@@:(e)` are distinct
                    // segment kinds).
                    FrameSegmentKind::ContextReturn
                        if matches!(
                            metadata,
                            SegmentMetadata::ContextReturn { assign_expr: None }
                        ) && is_standalone_statement(body, span) =>
                    {
                        self.warnings.push(ValidationError::new(
                            "W416",
                            format!(
                                "bare `@@:return` as a statement in {}/{} has no effect — it reads the return slot and discards it. Did you mean `@@:return(expr)` (set + exit), `@@:return = expr` (set), or `return` (native exit)?",
                                scope_outer, scope_inner
                            ),
                        ));
                    }
                    // E601: @@:self.method() — check method exists in interface
                    FrameSegmentKind::ContextSelfCall => {
                        if let SegmentMetadata::SelfCall { method, args } = metadata {
                            if let Some(iface_method) = interface_methods.get(method.as_str()) {
                                // E602: check argument count
                                let arg_count = self.count_args(args);
                                let expected = iface_method.params.len();
                                if arg_count != expected {
                                    self.errors.push(ValidationError::new(
                                        "E602",
                                        format!(
                                            "@@:self.{}() in {}/{} has {} arguments but interface expects {}",
                                            method, scope_outer, scope_inner, arg_count, expected
                                        )
                                    ));
                                }
                            } else if !actions.contains(method.as_str())
                                && !operations.contains(method.as_str())
                            {
                                // Not an interface method, action, or operation.
                                // `@@:self.<action|operation>(args)` is a valid direct
                                // call (RFC-0046) — the portable spelling that, on
                                // targets without a native `self` (Erlang/C), is the
                                // only way to call your own action/operation.
                                self.errors.push(ValidationError::new(
                                    "E601",
                                    format!(
                                        "@@:self.{}() in {}/{} — '{}' is not an interface method, action, or operation",
                                        method, scope_outer, scope_inner, method
                                    )
                                ));
                            }
                        }
                    }

                    // ContextSelf covers both bare `@@:self` (→ E603) and the
                    // RFC-0046 field form `@@:self.<field>` (→ resolve the member).
                    FrameSegmentKind::ContextSelf => {
                        if let SegmentMetadata::SelfField { field } = metadata {
                            // E609: `@@:self.<field>` must name a domain field or
                            // an interface method (the latter is the no-paren
                            // reference form). Unknown → reject; the symbol table
                            // has the answer at compile time (RFC-0046 D3).
                            if !domain_fields.contains(field)
                                && !interface_methods.contains_key(field.as_str())
                            {
                                self.errors.push(ValidationError::new(
                                    "E609",
                                    format!(
                                        "`@@:self.{}` in {}/{} references no known domain field or interface method",
                                        field, scope_outer, scope_inner
                                    ),
                                ));
                            }
                        } else {
                            // Bare `@@:self` — requires a member access.
                            self.errors.push(ValidationError::new(
                                "E603",
                                format!(
                                    "bare `@@:self` in {}/{} — `@@:self` requires a member access (e.g. `@@:self.method(args)`)",
                                    scope_outer, scope_inner
                                ),
                            ));
                        }
                    }

                    // E609: `@@:self.field.method()` (RFC-0046) — `field` must be
                    // a domain field. (Embed-vs-scalar is decided at codegen from
                    // the field's type; here we only confirm the field exists.)
                    FrameSegmentKind::ContextSelfFieldCall => {
                        if let SegmentMetadata::SelfFieldCall {
                            field,
                            method,
                            index,
                            ..
                        } = metadata
                        {
                            if !domain_fields.contains(field) {
                                self.errors.push(ValidationError::new(
                                    "E609",
                                    format!(
                                        "`@@:self.{}.…()` in {}/{} references no known domain field",
                                        field, scope_outer, scope_inner
                                    ),
                                ));
                            } else if index.is_some()
                                && matches!(
                                    target,
                                    crate::frame_c::visitors::TargetLanguage::Lua
                                        | crate::frame_c::visitors::TargetLanguage::C
                                )
                            {
                                // E617 (#159 round 3, Lua only): an indexed
                                // cross-system call whose element system can't
                                // be resolved lowers to a DOT call — legal Lua
                                // that silently passes the first argument as
                                // `self` (state corruption, no diagnostic at
                                // any later stage). Fail loudly instead.
                                // Resolution mirrors codegen: the unique
                                // system token in the declared type, else the
                                // unique OTHER system declaring the method.
                                let names = crate::frame_c::compiler::codegen::interface_gen::known_system_names();
                                let type_resolves = domain_field_types
                                    .get(field)
                                    .map(|t| {
                                        let is_word =
                                            |b: u8| b.is_ascii_alphanumeric() || b == b'_';
                                        let bytes = t.as_bytes();
                                        let mut hit: Option<&str> = None;
                                        let mut ambiguous = false;
                                        for sys in &names {
                                            let mut from = 0usize;
                                            while let Some(off) = t[from..].find(sys.as_str()) {
                                                let st = from + off;
                                                let en = st + sys.len();
                                                let lok = st == 0 || !is_word(bytes[st - 1]);
                                                let rok = en >= bytes.len() || !is_word(bytes[en]);
                                                if lok && rok {
                                                    match hit {
                                                        Some(prev) if prev != sys.as_str() => {
                                                            ambiguous = true
                                                        }
                                                        _ => hit = Some(sys.as_str()),
                                                    }
                                                    break;
                                                }
                                                from = en;
                                            }
                                        }
                                        hit.is_some() && !ambiguous
                                    })
                                    .unwrap_or(false);
                                let method_resolves =
                                    crate::frame_c::compiler::codegen::interface_gen::unique_system_with_interface_method(
                                        method,
                                        calling_system,
                                    )
                                    .is_some();
                                if !names.is_empty() && !type_resolves && !method_resolves {
                                    let (consequence, guidance) = match target {
                                        crate::frame_c::visitors::TargetLanguage::C => (
                                            "C would emit a verbatim member call (`self->field[i].method(...)`), which is invalid C (structs have no methods)",
                                            format!(
                                                "Name the element system directly in the declared type, e.g. `{field}: <System>*[N]` — framec emits the C declarator (`<System>* {field}[N];`), so no native typedef is needed (a typedef hides the element type from the type-ignorant lowering)."
                                            ),
                                        ),
                                        _ => (
                                            "Lua would emit a DOT call, which silently passes the first argument as `self` (state corruption)",
                                            format!(
                                                "Annotate the element type on the field, e.g. `{field}: <System>[] = {{}}` — the annotation is informational on Lua but drives the colon-dispatch lowering."
                                            ),
                                        ),
                                    };
                                    self.errors.push(ValidationError::new(
                                        "E617",
                                        format!(
                                            "indexed cross-system call `@@:self.{field}[…].{method}(…)` in {scope_outer}/{scope_inner} cannot resolve its element system — the field's declared type doesn't name one and `{method}` is declared by zero or multiple other systems. {consequence}. {guidance}"
                                        ),
                                    ));
                                }
                            }
                        }
                    }

                    // E604: bare @@:system without a recognized member
                    FrameSegmentKind::ContextSystemBare => {
                        self.errors.push(ValidationError::new(
                            "E604",
                            format!(
                                "bare `@@:system` in {}/{} — `@@:system` requires a member access (e.g. `@@:system.state.name`)",
                                scope_outer, scope_inner
                            ),
                        ));
                    }

                    // E608: @@:system.state without .name — reserved (RFC-0045)
                    FrameSegmentKind::ContextSystemStateReserved => {
                        self.errors.push(ValidationError::new(
                            "E608",
                            format!(
                                "`@@:system.state` in {}/{} is reserved for future use; use `@@:system.state.name` to read the current state name",
                                scope_outer, scope_inner
                            ),
                        ));
                    }

                    _ => {}
                }
            }
        }

        // Close the E604/E608 expression-context gap (RFC-0045): a reserved
        // `@@:system` / `@@:system.state` nested inside a return expression
        // (`@@:(…)`, `@@:return(…)`, `@@:return = …`) is not a top-level
        // segment, so the walk above can't see it. Re-scan each return
        // expression's inner text and raise there too.
        self.check_reserved_system_in_expr(&scan_result.regions, scope_outer, scope_inner, target);

        // W705: transition in a non-void handler without a preceding
        // `@@:(value)` may leak the return type's default (None /
        // Nil / null / 0) on the transition's execution path.
        //
        // Per `frame_language.md`: "Every transition is implicitly
        // followed by a `return` — code after a transition is
        // unreachable." The codegen's same-scope hoist makes the
        // simple shape `-> $X; @@:(value)` work (the @@:(value)
        // gets reordered before the bare return), but a
        // `@@:(value)` in an enclosing scope after the transition
        // remains genuinely unreachable on the transition path.
        //
        // Two safe shapes that suppress this warning:
        //   1. `@@:(value)` (or `@@:return(value)`) appears earlier
        //      in the body at an indent ≤ the transition's indent —
        //      `_return` was already set before the transition runs.
        //   2. `@@:(value)` immediately follows the transition at
        //      the same indent — the codegen hoists it before the
        //      bare return.
        //
        // The check is intentionally heuristic. It catches the
        // common "I wrote `@@:(value)` outside the if; why is it
        // returning Nil?" mistake (Issue #4 in FRAMEC_BUGS.md). It
        // can produce a false negative for sibling-block cases
        // where an earlier @@:(value) exists in a non-preceding
        // branch — that's accepted; the warning is meant to catch
        // the easy mistake without flagging legitimate patterns.
        if let Some(iface_method) = interface_methods.get(scope_inner) {
            // A handler "returns a value" if EITHER the interface
            // declares an explicit return type (`: int`) OR a default
            // return expression (`= "denied"`). Dynamic-typed targets
            // (Ruby, Lua, PHP, JS) commonly drop the type annotation
            // and rely on the default-expression form — `get_status()
            // = ""` is "returns a value, defaulting to empty string."
            let returns_value = {
                let has_type = match &iface_method.return_type {
                    Some(t) => {
                        let s = match t {
                            crate::frame_c::compiler::frame_ast::Type::Custom(s) => s.as_str(),
                            crate::frame_c::compiler::frame_ast::Type::Unknown => "",
                        };
                        !s.is_empty() && s != "void"
                    }
                    None => false,
                };
                has_type || iface_method.return_init.is_some()
            };
            // E606: `@@:(value)` (or `@@:return(value)`) in a handler
            // whose interface method is void. The write to `_return` has
            // no observable effect — the caller has no typed read path
            // for the value.
            //
            // RUST-ONLY: pre-Track-B `Box<dyn Any>` accepted this silently
            // on every backend, but Track B's per-event return enum on
            // the Rust target exposes it as a structural error (no enum
            // variant exists to write into). The other 16 backends still
            // use dynamic dispatch and tolerate the dead write — gating
            // this validator pass to Rust avoids breaking ~70 fixtures
            // across dynamic-typed targets where the pattern is benign.
            if !returns_value && matches!(target, crate::frame_c::visitors::TargetLanguage::Rust) {
                for r in scan_result.regions.iter() {
                    if let Region::FrameSegment {
                        kind: k,
                        metadata: m,
                        ..
                    } = r
                    {
                        // Three syntactic shapes write to `_return`:
                        //   1. `@@:(expr)` — concise (ContextReturnExpr)
                        //   2. `@@:return(expr)` — call form (ReturnCall),
                        //      but ONLY when it carries an expression. The
                        //      void form `@@:return()` (#141) is a pure early
                        //      exit that leaves the slot untouched, so it is
                        //      legal in a void handler and must NOT trigger
                        //      E606 (it has no value to land in a missing enum
                        //      variant).
                        //   3. `@@:return = expr;` — assignment form
                        //      (ContextReturn with assign_expr = Some)
                        // These need E606 on Rust when the interface method is
                        // void — the per-event return enum has no variant to
                        // write into.
                        let return_call_with_value = matches!(
                            (k, m),
                            (
                                FrameSegmentKind::ReturnCall,
                                SegmentMetadata::ReturnCall { expr }
                            ) if !expr.trim().is_empty()
                        );
                        let assigns_return = matches!(k, FrameSegmentKind::ContextReturnExpr)
                            || return_call_with_value
                            || matches!(
                                (k, m),
                                (
                                    FrameSegmentKind::ContextReturn,
                                    SegmentMetadata::ContextReturn {
                                        assign_expr: Some(_)
                                    }
                                )
                            );
                        if assigns_return {
                            self.errors.push(ValidationError::new(
                                "E606",
                                format!(
                                    "`@@:(value)` or `@@:return = value` in {}/{} — interface method `{}` is void on the Rust target, so writing to `_return` has no observable effect (and Track B's per-event return enum has no variant for it). Remove the `@@:(value)` / `@@:return = value` (or add a return type to `{}` in the interface).",
                                    scope_outer, scope_inner, scope_inner, scope_inner
                                ),
                            ));
                            break; // one error per handler is enough
                        }
                    }
                }
            }

            if returns_value {
                let frame_regs: Vec<&Region> = scan_result
                    .regions
                    .iter()
                    .filter(|r| matches!(r, Region::FrameSegment { .. }))
                    .collect();
                for (i, r) in frame_regs.iter().enumerate() {
                    if let Region::FrameSegment {
                        kind: FrameSegmentKind::Transition,
                        indent: t_indent,
                        ..
                    } = **r
                    {
                        // Check 1: any @@:(value) at indent ≤ t_indent earlier in body.
                        let preceded = frame_regs[..i].iter().any(|r2| {
                            if let Region::FrameSegment {
                                kind: k,
                                indent: i2,
                                ..
                            } = **r2
                            {
                                matches!(
                                    k,
                                    FrameSegmentKind::ContextReturnExpr
                                        | FrameSegmentKind::ReturnCall
                                ) && i2 <= t_indent
                            } else {
                                false
                            }
                        });
                        // Check 2: same-indent @@:(value) immediately following
                        // (codegen's same-scope hoist applies).
                        let immediately_followed = frame_regs
                            .get(i + 1)
                            .map(|r2| {
                                if let Region::FrameSegment {
                                    kind: k,
                                    indent: i2,
                                    ..
                                } = **r2
                                {
                                    matches!(
                                        k,
                                        FrameSegmentKind::ContextReturnExpr
                                            | FrameSegmentKind::ReturnCall
                                    ) && i2 == t_indent
                                } else {
                                    false
                                }
                            })
                            .unwrap_or(false);
                        if !preceded && !immediately_followed {
                            self.warnings.push(ValidationError::new(
                                "W705",
                                format!(
                                    "transition in {}/{} may leak the return type's default value \
                                     (None/Nil/null/0): no `@@:(value)` precedes the transition at \
                                     this scope or any enclosing scope, and no same-scope `@@:(value)` \
                                     immediately follows it. The transition's implicit `return` will \
                                     short-circuit before any later `@@:(value)` in an outer scope. \
                                     Fix: place `@@:(value)` before the transition, or use \
                                     `@@:return(value)` at the transition site.",
                                    scope_outer, scope_inner
                                ),
                            ));
                        }
                    }
                }
            }
        }
    }
}

/// True when the segment at `span` is alone on its logical statement: the
/// bytes from the previous statement delimiter up to the segment start, and
/// from the segment end to the next statement delimiter, are whitespace only.
///
/// Statement delimiters are `\n`, `;`, `{`, and `}` — the boundaries that
/// separate native statements in every target Frame emits into. This is used
/// for W416 (#130): a bare `@@:return` read that stands alone as a statement
/// is a no-op, but the same read embedded in an expression (`x = @@:return + 1`)
/// is consumed and must not warn. Walking to a real delimiter (rather than only
/// the previous newline) keeps `f(); @@:return` on one line classified as
/// standalone while never misreading the embedded-expression case.
fn is_standalone_statement(body: &[u8], span: &RegionSpan) -> bool {
    let is_delim = |b: u8| b == b'\n' || b == b';' || b == b'{' || b == b'}';
    let is_ws = |b: u8| b == b' ' || b == b'\t' || b == b'\r';

    // Left: walk back from the segment start. Every byte until a delimiter
    // (or the body start) must be whitespace.
    let mut p = span.start;
    while p > 0 {
        let b = body[p - 1];
        if is_delim(b) {
            break;
        }
        if !is_ws(b) {
            return false;
        }
        p -= 1;
    }

    // Right: walk forward from the segment end. Every byte until a delimiter
    // (or the body end) must be whitespace.
    let mut q = span.end;
    while q < body.len() {
        let b = body[q];
        if is_delim(b) {
            break;
        }
        if !is_ws(b) {
            return false;
        }
        q += 1;
    }

    true
}
