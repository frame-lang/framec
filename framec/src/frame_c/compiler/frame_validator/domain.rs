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
                    while let Some(rel) = code[search_from..].find(prefix.as_str()) {
                        let abs = search_from + rel;
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
                        if init_references_param(init_text, &one) {
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
        let requires_explicit_type = matches!(target, C | Cpp | Java | Go | Rust | CSharp | TypeScript);
        if !requires_explicit_type {
            return;
        }
        for var in &system.domain {
            if matches!(var.var_type, Type::Unknown) {
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
        }
    }
}
