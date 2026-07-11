//! Domain-section validation: const-field shape, static-vs-domain placement,
//! domain-field initializer shape, and per-target type-string checks.
//!
//! These methods enforce the rules a Frame system's `domain:` section must
//! follow before codegen runs — most importantly that `const` fields are
//! never reassigned (E615), that `static` lives on the system (not the
//! state) and `domain` lives on the state (not the system), and that
//! initializer expressions match the declared (or required) target type.

use super::{FrameValidator, ValidationError};
use crate::frame_c::compiler::codegen::system_codegen::init_references_param;
use crate::frame_c::compiler::frame_ast::*;
use std::collections::HashSet;

/// E609 helper (FRAMEC_BUGS #37): bare Python-ish container type names have
/// no meaning as a type on any statically-typed target — Frame passes type
/// strings through verbatim, so `domain: xs: list` emits `pub xs: list`
/// (invalid Rust). Returns a per-language hint for the few unambiguous
/// pseudo-types; `None` for everything else.
///
/// Only the exact bare lowercase names are matched — a real static-target
/// type (`List`/`Map` in Java/Kotlin, `std::vector`/`std::map` in C++,
/// `Vec`/`HashMap` in Rust) is capitalized or namespaced and won't match,
/// so this never flags legitimate native types.
fn static_target_pseudo_type(t: &str) -> Option<&'static str> {
    match t.trim() {
        "list" => Some("your target's list/array type (e.g. `Vec<T>`, `std::vector<T>`, `List<T>`, `[]T`)"),
        "dict" => Some("your target's map type (e.g. `HashMap<K, V>`, `std::map<K, V>`, `Map<K, V>`, `map[K]V`)"),
        "set" => Some("your target's set type (e.g. `HashSet<T>`, `std::set<T>`, `Set<T>`)"),
        "tuple" => Some("your target's tuple or a named struct type"),
        _ => None,
    }
}

impl FrameValidator {
    /// E615: Direct assignment to a `const` domain field inside a handler
    /// body. Catches the obvious per-target self-access patterns; the target
    /// language compiler catches anything else via the emitted
    /// `final` / `readonly` / `const` / `val` / `let` keyword.
    pub(super) fn validate_const_field_assignments(&mut self, system: &SystemAst) {
        let const_fields: Vec<&str> = system
            .domain
            .iter()
            .filter(|v| v.is_const)
            .map(|v| v.name.as_str())
            .collect();
        if const_fields.is_empty() {
            return;
        }

        let machine = match &system.machine {
            Some(m) => m,
            None => return,
        };

        for state in &machine.states {
            if let Some(ref h) = state.enter {
                self.scan_body_for_const_assigns(&h.body, &const_fields, &system.name, "$>");
            }
            if let Some(ref h) = state.exit {
                self.scan_body_for_const_assigns(&h.body, &const_fields, &system.name, "$<");
            }
            for h in &state.handlers {
                self.scan_body_for_const_assigns(&h.body, &const_fields, &system.name, &h.event);
            }
        }
    }

    fn scan_body_for_const_assigns(
        &mut self,
        body: &HandlerBody,
        const_fields: &[&str],
        system_name: &str,
        event_name: &str,
    ) {
        for stmt in &body.statements {
            let code = match stmt {
                Statement::NativeCode(s) => s.as_str(),
                _ => continue,
            };
            for field in const_fields {
                // Per-target self-access prefixes that resolve to the system
                // instance: catches `self.x =`, `this->x =`, `@x =`, etc.
                let prefixes = [
                    format!("self.{}", field),
                    format!("this.{}", field),
                    format!("self->{}", field),
                    format!("this->{}", field),
                    format!("$this->{}", field),
                    format!("@{}", field),
                ];
                let mut flagged = false;
                for prefix in &prefixes {
                    let mut search_from = 0usize;
                    // #149: only a `self.field =` in a CODE region is a real
                    // assignment — a match inside a native string literal
                    // (`log("self.count = done")`) or comment is not. Scan via
                    // the target's SyntaxSkipper. `search_from` always resumes at
                    // a code position (just past a prior code-region match).
                    while let Some(abs) =
                        crate::frame_c::compiler::codegen::codegen_utils::find_outside_strings_and_comments_from(
                            code,
                            self.target,
                            prefix.as_str(),
                            search_from,
                        )
                    {
                        let after = &code[abs + prefix.len()..];
                        let trimmed = after.trim_start();
                        // Match `=` or augmented assignment, but NOT `==`.
                        let is_assign = (trimmed.starts_with('=')
                            && !trimmed.starts_with("==")
                            && !trimmed.starts_with("=>"))
                            || trimmed.starts_with("+=")
                            || trimmed.starts_with("-=")
                            || trimmed.starts_with("*=")
                            || trimmed.starts_with("/=")
                            || trimmed.starts_with("%=");
                        if is_assign {
                            // Reject access to a sub-field: `self.x.foo = ...`
                            // (the assignment is to `foo`, not to `x`).
                            // The trim already handled whitespace before `=`,
                            // so any `.` immediately after the prefix means
                            // the user is accessing a member of the field,
                            // not assigning to the field itself.
                            let raw_after = &code[abs + prefix.len()..];
                            if !raw_after.starts_with('.') && !raw_after.starts_with("->") {
                                self.errors.push(
                                    ValidationError::new(
                                        "E615",
                                        format!(
                                            "Assignment to const domain field '{}' \
                                             in system '{}' handler '{}'",
                                            field, system_name, event_name
                                        ),
                                    )
                                    .with_span(body.span.clone()),
                                );
                                flagged = true;
                                break;
                            }
                        }
                        search_from = abs + prefix.len();
                    }
                    if flagged {
                        break;
                    }
                }
            }
        }
    }

    /// E420: `static` is only valid on operations
    pub(super) fn validate_static_placement(&mut self, system: &SystemAst) {
        for method in &system.interface {
            if method.is_static {
                self.errors.push(
                    ValidationError::new(
                        "E420",
                        format!(
                            "'static' is not valid on interface method '{}' in system '{}'. \
                             Only operations can be static.",
                            method.name, system.name
                        ),
                    )
                    .with_span(method.span.clone()),
                );
            }
        }
        for action in &system.actions {
            if action.is_static {
                self.errors.push(
                    ValidationError::new(
                        "E420",
                        format!(
                            "'static' is not valid on action '{}' in system '{}'. \
                             Only operations can be static.",
                            action.name, system.name
                        ),
                    )
                    .with_span(action.span.clone()),
                );
            }
        }
    }

    /// E613: Domain field shadows system parameter
    /// E614: Duplicate domain field name.
    /// W706: `const` domain field seeded from a required (no-default)
    /// system param. `@@!Foo()` and persist `@@[load]` / restore skip
    /// the system's initialization, so the `const` field can't be
    /// seeded — on C++ the bare ctor takes the param so `Foo()` won't
    /// type-check; on other backends the field silently picks up the
    /// type's zero value, which is worse (silent wrong behaviour).
    /// Tracked as A8/A1 in the 4.2 plan; this warning surfaces the
    /// gap at validate time so the user can choose a fix before the
    /// codegen output bites them.
    pub(super) fn validate_domain_fields(&mut self, system: &SystemAst) {
        let _param_names: HashSet<&str> = system.params.iter().map(|p| p.name.as_str()).collect();
        let mut seen: HashSet<&str> = HashSet::new();

        // Collect required (no-default) param names once — the W706
        // scan tests every `const` field's initializer against this set.
        let required_param_names: Vec<String> = system
            .params
            .iter()
            .filter(|p| p.default.is_none())
            .map(|p| p.name.clone())
            .collect();

        for var in &system.domain {
            // E614: Duplicate domain field name
            if !seen.insert(&var.name) {
                self.errors.push(
                    ValidationError::new(
                        "E614",
                        format!(
                            "Duplicate domain field '{}' in system '{}'",
                            var.name, system.name
                        ),
                    )
                    .with_span(var.span.clone()),
                );
            }

            // Note: Domain fields intentionally share names with Domain-kind system
            // params (the param initializes the field). E613 is reserved for future
            // use if we want to warn about non-Domain param shadowing.

            // W706: const + required-param-seeded field is a no-init hazard.
            if var.is_const && !required_param_names.is_empty() {
                if let Some(init_text) = &var.initializer_text {
                    // Per-param scan so we can name the specific param
                    // in the warning. init_references_param is the
                    // same word-boundary checker codegen uses elsewhere.
                    for param_name in &required_param_names {
                        let one = vec![param_name.clone()];
                        if init_references_param(init_text, &one, self.target) {
                            self.warnings.push(
                                ValidationError::new(
                                    "W706",
                                    format!(
                                        "system '{sys}' has a `const` domain field '{field}' \
                                         initialized from required (no-default) system param \
                                         '{param}'. `@@!{sys}()` (no-init allocation) and \
                                         `@@[load]` / restore skip the system's initialization, \
                                         so the `const` field cannot be seeded — on C++ the bare \
                                         constructor requires the param so `{sys}()` won't \
                                         type-check; on other backends the field silently picks \
                                         up the type's zero value. Fix options: (1) give the \
                                         param a default — `{param}: T = <value>`; (2) drop the \
                                         `const` so the field is settable post-construction; \
                                         or (3) initialize the field with a literal instead of \
                                         the param. See RFC-0017's \"Generated calls\" section \
                                         and the 4.2 plan note on A1.",
                                        sys = system.name,
                                        field = var.name,
                                        param = param_name
                                    ),
                                )
                                .with_span(var.span.clone()),
                            );
                            break; // one warning per field; don't spam if the init refs multiple required params.
                        }
                    }
                }
            }
        }
    }

    /// E605: targets without struct-field type inference require an
    /// explicit `: type` annotation on every domain field. Without
    /// it, framec used to emit nonsense (Rust: `pub field: ()`
    /// (unit), Java/C#/etc.: cascading errors that don't trace to the
    /// missing annotation). Rejecting here gives the user a clear,
    /// source-level diagnostic instead.
    ///
    /// Languages where struct-field initializers DO infer the type
    /// (Kotlin `val x = 0`, Swift `var x = 0`, Dart `var x = 0`)
    /// and dynamic languages without static types (Python, Lua,
    /// JS, Ruby, PHP, GDScript) skip the check — the bare-init form
    /// is valid Frame syntax there.
    pub(super) fn validate_domain_types(
        &mut self,
        system: &SystemAst,
        target: crate::frame_c::visitors::TargetLanguage,
    ) {
        use crate::frame_c::visitors::TargetLanguage::*;
        // Targets where struct-field declarations REQUIRE an explicit
        // type (no inference from initializer at the field-decl site).
        // Rust: `struct { x = 0 }` is a parse error. C#: same.
        // TypeScript: class-field `x = 0` IS inferred, BUT framec's
        // codegen emits a structurally-typed shape that doesn't
        // exercise the inference — surface the gap to the user
        // rather than emit ambiguous output.
        let requires_explicit_type =
            matches!(target, C | Cpp | Java | Go | Rust | CSharp | TypeScript);
        if !requires_explicit_type {
            return;
        }
        for var in &system.domain {
            match &var.var_type {
                Type::Unknown => {
                    self.errors.push(
                        ValidationError::new(
                            "E605",
                            format!(
                                "domain field '{}' in system '{}' missing type annotation. \
                                 Frame's canonical domain form is `name: type = init`. \
                                 For target '{:?}', framec cannot infer struct-field types \
                                 from initializers — the explicit annotation is required. \
                                 Add `: <type>` between the field name and `=`. \
                                 See docs/frame_language.md § Domain Section.",
                                var.name, system.name, target
                            ),
                        )
                        .with_span(var.span.clone()),
                    );
                }
                // #37 is Rust-only: `list`/`dict` ARE supported domain types on
                // C (and dynamic targets) via the runtime's list/dict helpers
                // (e.g. C's `_persist_pack_field_list`); only Rust lacks the
                // mapping and emits the verbatim `pub xs: list` (invalid). Scope
                // the rejection to Rust so those legitimately-supported fixtures
                // still compile.
                Type::Custom(s) if matches!(target, Rust) => {
                    if let Some(hint) = static_target_pseudo_type(s) {
                        self.errors.push(
                            ValidationError::new(
                                "E609",
                                format!(
                                    "domain field '{}' in system '{}' has type '{}', which the \
                                     Rust backend does not map to a real type (Frame passes type \
                                     names through verbatim, so it would emit the invalid \
                                     `pub {}: {}`). Write {} instead. \
                                     See docs/frame_language.md § Types and Expressions.",
                                    var.name, system.name, s, var.name, s, hint
                                ),
                            )
                            .with_span(var.span.clone()),
                        );
                    }
                }
                Type::Custom(_) => {}
            }
        }
    }

    /// E606: statically-typed targets require an explicit type on every
    /// interface-method parameter. Unlike domain fields (E605), a
    /// parameter has no initializer to infer from, so this also applies
    /// to Kotlin/Swift (which *can* infer a field type from its init).
    /// Frame has no type system — types pass through verbatim — so for
    /// these targets framec cannot synthesize a parameter type; the
    /// annotation is mandatory. (Pre-FRAMEC_BUGS-#37 this silently
    /// defaulted to an `Any`/`object` placeholder via the per-backend
    /// type table; with the table removed, an untyped param would leak
    /// invalid code, so we reject it up front.)
    pub(super) fn validate_interface_param_types(
        &mut self,
        system: &SystemAst,
        target: crate::frame_c::visitors::TargetLanguage,
    ) {
        use crate::frame_c::visitors::TargetLanguage::*;
        let requires_explicit_type =
            matches!(target, C | Cpp | Java | Go | Rust | CSharp | Kotlin | Swift);
        if !requires_explicit_type {
            return;
        }
        let mut flag = |kind: &str,
                        owner: &str,
                        pname: &str,
                        ptype: &Type,
                        span: &Span,
                        errs: &mut Vec<ValidationError>| {
            match ptype {
                Type::Unknown => {
                    errs.push(
                        ValidationError::new(
                            "E606",
                            format!(
                                "{} '{}' parameter '{}' is missing a type annotation. \
                                 Frame has no type system — type names pass through \
                                 verbatim — and for the statically-typed target '{:?}' \
                                 framec cannot synthesize a parameter type. Write \
                                 `{}: <your target's type>` (e.g. `int`, `i64`, \
                                 `std::string`). See docs/frame_language.md \
                                 § Types and Expressions.",
                                kind, owner, pname, target, pname
                            ),
                        )
                        .with_span(span.clone()),
                    );
                }
                // #37 Rust-only — see the domain-field branch above.
                Type::Custom(s) if matches!(target, Rust) => {
                    if let Some(hint) = static_target_pseudo_type(s) {
                        errs.push(
                            ValidationError::new(
                                "E609",
                                format!(
                                    "{} '{}' parameter '{}' has type '{}', which the Rust backend \
                                     does not map to a real type (it would emit the verbatim, \
                                     invalid `{}`). Write {} instead. \
                                     See docs/frame_language.md § Types and Expressions.",
                                    kind, owner, pname, s, s, hint
                                ),
                            )
                            .with_span(span.clone()),
                        );
                    }
                }
                Type::Custom(_) => {}
            }
        };
        // Interface-declared methods.
        for method in &system.interface {
            for param in &method.params {
                flag(
                    "interface method",
                    &method.name,
                    &param.name,
                    &param.param_type,
                    &param.span,
                    &mut self.errors,
                );
            }
        }
        // Event handlers in the machine (an event need not be declared in
        // `interface:` — the handler signature defines it, and its params
        // become the generated public method / FrameEvent fields).
        if let Some(machine) = &system.machine {
            for state in &machine.states {
                // State parameters: `$S(x: type)` — become typed compartment
                // fields / constructor args.
                for sp in &state.params {
                    flag(
                        "state",
                        &state.name,
                        &sp.name,
                        &sp.param_type,
                        &sp.span,
                        &mut self.errors,
                    );
                }
                // Event handler params.
                for handler in &state.handlers {
                    for param in &handler.params {
                        flag(
                            "event handler",
                            &handler.event,
                            &param.name,
                            &param.param_type,
                            &param.span,
                            &mut self.errors,
                        );
                    }
                }
                // Enter/exit lifecycle params: `$>(name: type)` / `<$(name: type)`.
                if let Some(enter) = &state.enter {
                    for p in &enter.params {
                        flag(
                            "$> enter handler in state",
                            &state.name,
                            &p.name,
                            &p.param_type,
                            &p.span,
                            &mut self.errors,
                        );
                    }
                }
                if let Some(exit) = &state.exit {
                    for p in &exit.params {
                        flag(
                            "<$ exit handler in state",
                            &state.name,
                            &p.name,
                            &p.param_type,
                            &p.span,
                            &mut self.errors,
                        );
                    }
                }
            }
        }
    }

    /// E752 (RFC-0055 R1): in a **persisted** system, every persisted field must
    /// declare a type on the targets where R1 is MUST — Regime A (statically typed)
    /// and Regime C (dynamic non-reflective: Lua, GDScript) — because there the
    /// declared type is the type-identity source for faithful restore and for a
    /// complete drift fingerprint. Regime B (Python/Ruby/PHP/JS/TS) supplies the
    /// type from a runtime tag, so a declared type is only RECOMMENDED there and is
    /// not checked here.
    ///
    /// Scoped to NOT overlap the codegen rules E605 (domain fields on
    /// C/Cpp/Java/Go/Rust/CSharp/TypeScript) and E606 (all args on
    /// C/Cpp/Java/Go/Rust/CSharp/Kotlin/Swift). The genuinely-uncovered persisted
    /// fields this fills, persist-gated:
    ///   - **state variables** (`$.x`) — no existing rule, on every Regime A/C target;
    ///   - **domain fields** on Kotlin/Swift/Dart/Lua/GDScript (E605's Regime A/C gap);
    ///   - **state / enter / exit args** on Dart/Lua/GDScript (E606's Regime A/C gap).
    pub(super) fn validate_persist_field_types(
        &mut self,
        system: &SystemAst,
        target: crate::frame_c::visitors::TargetLanguage,
    ) {
        use crate::frame_c::visitors::TargetLanguage::*;
        // R1 governs *persisted* fields only.
        if system.persist_attr.is_none() {
            return;
        }
        // R1 is MUST wherever the target CANNOT enumerate its own module's
        // classes at restore, so the declared field type is the only
        // reflection-free registry seed / drift-fingerprint source:
        //   - static targets (Rust/Go/Java/Kotlin/C#/Swift/Dart/C/C++)
        //   - Lua/GDScript (dynamic, but no module-class enumeration)
        //   - JS/TS (ES modules don't expose top-level class decls; #182)
        // Python/Ruby/PHP are EXEMPT — they enumerate module classes at
        // restore (e.g. Python walks `vars(_mod)`), so a declared type is
        // genuinely optional there (RFC-0055 R1 RECOMMENDED, not MUST).
        if !matches!(
            target,
            Rust | Go
                | Java
                | Kotlin
                | CSharp
                | Swift
                | Dart
                | C
                | Cpp
                | Lua
                | GDScript
                | JavaScript
                | TypeScript
        ) {
            return;
        }
        let domain_covered = matches!(target, C | Cpp | Java | Go | Rust | CSharp | TypeScript);
        let args_covered = matches!(target, C | Cpp | Java | Go | Rust | CSharp | Kotlin | Swift);

        let flag = |kind: &str, owner: &str, span: &Span, errs: &mut Vec<ValidationError>| {
            errs.push(
                ValidationError::new(
                    "E752",
                    format!(
                        "persisted {} '{}' in system '{}' is missing a type annotation. \
                         For target '{:?}' the declared type is the type-identity source for \
                         faithful restore and drift detection (RFC-0055 R1) — this target \
                         cannot enumerate its own module's classes at restore, so framec \
                         cannot reconstruct or fingerprint an untyped persisted field. \
                         Write `{}: <type>`. See docs/rfcs/rfc-0055.md § The contract.",
                        kind, owner, system.name, target, owner
                    ),
                )
                .with_span(span.clone()),
            );
        };

        // Domain fields — only where E605 does not already require them.
        if !domain_covered {
            for var in &system.domain {
                if var.attributes.iter().any(|a| a.name == "no_persist") {
                    continue;
                }
                if matches!(var.var_type, Type::Unknown) {
                    flag("domain field", &var.name, &var.span, &mut self.errors);
                }
            }
        }

        if let Some(machine) = &system.machine {
            for state in &machine.states {
                // State variables ($.x) — no existing rule, every Regime A/C target.
                for sv in &state.state_vars {
                    if matches!(sv.var_type, Type::Unknown) {
                        flag("state variable", &sv.name, &sv.span, &mut self.errors);
                    }
                }
                // State / enter / exit args — only where E606 does not already cover them.
                if !args_covered {
                    for sp in &state.params {
                        if matches!(sp.param_type, Type::Unknown) {
                            flag("state arg", &sp.name, &sp.span, &mut self.errors);
                        }
                    }
                    if let Some(enter) = &state.enter {
                        for p in &enter.params {
                            if matches!(p.param_type, Type::Unknown) {
                                flag("enter arg", &p.name, &p.span, &mut self.errors);
                            }
                        }
                    }
                    if let Some(exit) = &state.exit {
                        for p in &exit.params {
                            if matches!(p.param_type, Type::Unknown) {
                                flag("exit arg", &p.name, &p.span, &mut self.errors);
                            }
                        }
                    }
                }
            }
        }
    }
}
