//! `=> $^` HSM parent forward expansion.
//!
//! `=> $^` calls the parent state's handler for the same event,
//! threading the in-flight event up the HSM ancestor chain.
//! Implementation shape per backend:
//!
//! - **Per-handler dynamic backends** (Python/GDScript with
//!   `ctx.per_handler`, JS/TS/Dart, Ruby, Lua, PHP) — call
//!   the parent's `_state_<Parent>` method with the
//!   compartment shifted up one level so `$.varName` reads
//!   resolve in the parent's compartment scope.
//! - **Single-method dynamic** (Python/GDScript without
//!   per_handler) — dispatch via `__route_to_state` so the
//!   event lands on the parent's handler entry.
//! - **Static-typed** (Java/Kotlin/Swift/C#/Go/C/C++) — direct
//!   call to the parent state method; the compartment chain is
//!   walked through fields.
//! - **Rust** — delegates to
//!   `super::super::super::rust_system::rust_parent_forward`.
//! - **Erlang** — returns `{keep_state, Data}` so gen_statem
//!   replays via the next-event action queue.
//!
//! When `ctx.parent_state` is `None` (defensive — shouldn't
//! happen in a validated HSM), each backend emits a comment-
//! tagged bare `return` so the handler exits cleanly without a
//! crash.

use super::super::codegen_utils::{to_snake_case, HandlerContext};
use crate::frame_c::compiler::native_region_scanner::{RegionSpan, SegmentMetadata};
use crate::frame_c::visitors::TargetLanguage;

/// Expand `=> $^` (forward to parent) into `(body, terminator)`. A forward is
/// NOT a terminal transition — the parent handler is invoked and control
/// returns — so the terminator is always `""`. The no-parent fallback arms do
/// emit a bare `return`, but with a trailing comment, so they too are the whole
/// body with no separately-hoistable terminator. Returning `(body, "")`
/// preserves that (#169): the orchestrator appends nothing.
pub(super) fn expand_forward(
    segment_text: &str,
    indent: usize,
    lang: TargetLanguage,
    ctx: &HandlerContext,
    metadata: &SegmentMetadata,
) -> (String, &'static str) {
    let indent_str = " ".repeat(indent);

    // An explicit `=> $^` forward inside an async handler must be awaited just
    // like a handler dispatch — an un-awaited parent forward drops the return
    // value (Python: coroutine never awaited -> None) and mistypes in brace
    // langs. Prefix-await set mirrors state_dispatch's `aw`; Rust uses a
    // `.await` suffix (applied in rust_parent_forward); Kotlin (bare suspend) /
    // Java / Go and the non-async targets take no await keyword.
    let aw = if ctx.system_is_async {
        match lang {
            TargetLanguage::Cpp => "co_await ",
            TargetLanguage::Python3
            | TargetLanguage::TypeScript
            | TargetLanguage::JavaScript
            | TargetLanguage::Swift
            | TargetLanguage::CSharp
            | TargetLanguage::Dart
            | TargetLanguage::GDScript => "await ",
            _ => "",
        }
    } else {
        ""
    };

    // HSM forward: call parent state's handler for the same event
    let body = if let Some(ref parent) = ctx.parent_state {
        let forward = match lang {
            // Python/TypeScript: call _state_Parent(__e) to dispatch via unified state method
            TargetLanguage::Python3 | TargetLanguage::GDScript => {
                if ctx.per_handler {
                    // Per-handler architecture: shift compartment up one
                    // level at the forward site so the parent dispatcher
                    // sees its own compartment as the param.
                    format!(
                        "{}self._state_{}(__e, compartment.parent_compartment)",
                        indent_str, parent
                    )
                } else {
                    format!("{}self._state_{}(__e)", indent_str, parent)
                }
            }
            TargetLanguage::TypeScript | TargetLanguage::Dart | TargetLanguage::JavaScript => {
                if ctx.per_handler {
                    // Dart and TypeScript are null-safe; assert
                    // non-null with `!`. JavaScript ignores the
                    // postfix annotation in runtime semantics —
                    // keep it bare.
                    let bang = if matches!(lang, TargetLanguage::Dart | TargetLanguage::TypeScript)
                    {
                        "!"
                    } else {
                        ""
                    };
                    format!(
                        "{}this._state_{}(__e, compartment.parent_compartment{});",
                        indent_str, parent, bang
                    )
                } else {
                    format!("{}this._state_{}(__e);", indent_str, parent)
                }
            }
            // Rust: call parent state router (not specific handler) to dispatch via match
            TargetLanguage::Rust => super::super::rust_system::rust_parent_forward(
                &indent_str,
                parent,
                ctx.system_is_async,
            ),
            // C: call System_state_Parent(self, __e, parent_compartment)
            // — per-handler architecture shifts the compartment up
            // one level at the forward site.
            TargetLanguage::C => {
                if ctx.per_handler {
                    format!(
                        "{}{}_state_{}(self, __e, compartment->parent_compartment);",
                        indent_str, ctx.system_name, parent
                    )
                } else {
                    format!(
                        "{}{}_state_{}(self, __e);",
                        indent_str, ctx.system_name, parent
                    )
                }
            }
            // C++: call _state_Parent(__e, compartment->parent_compartment)
            // — forward is not terminal, no return.
            TargetLanguage::Cpp => {
                if ctx.per_handler {
                    format!(
                        "{}_state_{}(__e, compartment->parent_compartment);",
                        indent_str, parent
                    )
                } else {
                    format!("{}_state_{}(__e);", indent_str, parent)
                }
            }
            // Java: per-handler architecture passes compartment as
            // second arg; shift up one level at the forward site.
            TargetLanguage::Java => {
                if ctx.per_handler {
                    format!(
                        "{}_state_{}(__e, compartment.parent_compartment);",
                        indent_str, parent
                    )
                } else {
                    format!("{}_state_{}(__e);", indent_str, parent)
                }
            }
            TargetLanguage::CSharp => {
                if ctx.per_handler {
                    format!(
                        "{}_state_{}(__e, compartment.parent_compartment);",
                        indent_str, parent
                    )
                } else {
                    format!("{}_state_{}(__e);", indent_str, parent)
                }
            }
            TargetLanguage::Kotlin => {
                if ctx.per_handler {
                    format!(
                        "{}_state_{}(__e, compartment.parent_compartment!!)",
                        indent_str, parent
                    )
                } else {
                    format!("{}_state_{}(__e)", indent_str, parent)
                }
            }
            TargetLanguage::Swift => {
                if ctx.per_handler {
                    format!(
                        "{}_state_{}(__e, compartment.parent_compartment!)",
                        indent_str, parent
                    )
                } else {
                    format!("{}_state_{}(__e)", indent_str, parent)
                }
            }
            // Go: call s._state_Parent(__e) — forward is not terminal, no return
            TargetLanguage::Go => {
                if ctx.per_handler {
                    format!(
                        "{}s._state_{}(__e, compartment.parentCompartment)",
                        indent_str, parent
                    )
                } else {
                    format!("{}s._state_{}(__e)", indent_str, parent)
                }
            }
            TargetLanguage::Php => {
                if ctx.per_handler {
                    format!(
                        "{}$this->_state_{}($__e, $compartment->parent_compartment);",
                        indent_str, parent
                    )
                } else {
                    format!("{}$this->_state_{}($__e);", indent_str, parent)
                }
            }
            TargetLanguage::Ruby => {
                if ctx.per_handler {
                    format!(
                        "{}_state_{}(__e, compartment.parent_compartment)",
                        indent_str, parent
                    )
                } else {
                    format!("{}_state_{}(__e)", indent_str, parent)
                }
            }
            TargetLanguage::Lua => {
                if ctx.per_handler {
                    format!(
                        "{}self:_state_{}(__e, compartment.parent_compartment)",
                        indent_str, parent
                    )
                } else {
                    format!("{}self:_state_{}(__e)", indent_str, parent)
                }
            }
            TargetLanguage::Graphviz => unreachable!(),
        };
        // Prefix-await langs: splice the await in after the leading indent (Rust
        // took its `.await` suffix in the helper above; empty aw is a no-op).
        if aw.is_empty() {
            forward
        } else {
            format!("{}{}{}", indent_str, aw, &forward[indent_str.len()..])
        }
    } else {
        // No parent state - just return (shouldn't happen in valid HSM)
        match lang {
            TargetLanguage::Python3 | TargetLanguage::GDScript => {
                format!("{}return  # Forward to parent (no parent)", indent_str)
            }
            TargetLanguage::Ruby
            | TargetLanguage::Kotlin
            | TargetLanguage::Swift
            | TargetLanguage::Lua => {
                format!("{}return // Forward to parent (no parent)", indent_str)
            }
            TargetLanguage::TypeScript
            | TargetLanguage::JavaScript
            | TargetLanguage::Rust
            | TargetLanguage::Dart
            | TargetLanguage::C
            | TargetLanguage::Cpp
            | TargetLanguage::Java
            | TargetLanguage::CSharp
            | TargetLanguage::Go
            | TargetLanguage::Php => {
                format!("{}return; // Forward to parent (no parent)", indent_str)
            }
            TargetLanguage::Graphviz => unreachable!(),
        }
    };
    (body, "")
}
