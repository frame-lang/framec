//! `push$` / `pop$` modal-stack frame segment expansion.
//!
//! Two arms:
//!
//! - `expand_stack_push` — RFC-0008 `push$` (with optional
//!   transition target). Saves a REFERENCE to the current
//!   compartment on the state stack; the GC backends do a
//!   direct assignment, C/C++ do ref-count increments, Rust
//!   uses `clone` for bare push or `mem::replace` for
//!   push-with-transition.
//! - `expand_stack_pop` — bare `pop$` (no transition). Just
//!   pops the top of the stack and discards. The `-> pop$`
//!   form (pop-with-transition) is handled by
//!   `pop_transition::generate_pop_transition`.

use super::super::codegen_utils::{to_snake_case, HandlerContext};
use crate::frame_c::compiler::native_region_scanner::{RegionSpan, SegmentMetadata};
use crate::frame_c::visitors::TargetLanguage;

/// Expand `push$` (RFC-0008) into `(body, terminator)`. A bare `push$` (no
/// `$State`) just saves the current compartment and is NOT terminal, so its
/// terminator is `""`; a `push$ $State` push-transition exits the handler with
/// the language's [`transition_terminator`](super::utility::transition_terminator).
pub(super) fn expand_stack_push(
    segment_text: &str,
    indent: usize,
    lang: TargetLanguage,
    ctx: &HandlerContext,
    metadata: &SegmentMetadata,
) -> (String, &'static str) {
    let indent_str = " ".repeat(indent);

    // #211: the node now carries the WHOLE transition, not just its name.
    let transition: Option<&SegmentMetadata> = match metadata {
        SegmentMetadata::StackPush {
            transition: Some(t),
        } => Some(&**t),
        _ => None,
    };
    let target = match transition {
        Some(SegmentMetadata::Transition { target_state, .. }) => target_state.clone(),
        _ => String::new(),
    };
    // Does the transition carry args? If so, this file cannot emit it — the
    // transition expander already knows how to write state/enter/exit args on
    // every target, and duplicating that here is what lost them in the first place
    // (RFC-0056 P11: one emitter, not two).
    let has_args = matches!(
        transition,
        Some(SegmentMetadata::Transition {
            state_args: Some(_),
            ..
        }) | Some(SegmentMetadata::Transition {
            enter_args: Some(_),
            ..
        }) | Some(SegmentMetadata::Transition {
            exit_args: Some(_),
            ..
        })
    );
    if has_args {
        // Emit the push, then DELEGATE the transition. `push$` saves the current
        // compartment; the transition then does exactly what it does anywhere else.
        let push_only = push_compartment_line(&indent_str, lang, ctx);
        let (t_body, t_term) = super::transition::expand_transition(
            segment_text,
            indent,
            lang,
            ctx,
            transition.expect("has_args implies Some"),
        );
        return (format!("{}\n{}", push_only, t_body), t_term);
    }

    // push$ saves a REFERENCE to the current compartment on the
    // state stack — not a copy. In GC languages this is a direct
    // assignment. In C it's a pointer save (ownership transfers to
    // stack on push-with-transition). In C++ it's a shared_ptr
    // copy (ref count increment). In Rust, clone is required for
    // bare push$ (ownership model) but push-with-transition uses
    // mem::replace (ownership transfer). pop$ restores the saved
    // reference as the current compartment.
    let body = match lang {
        TargetLanguage::Python3 => {
            let push_code = format!("{}self._state_stack.append(self.__compartment)", indent_str);
            if !target.is_empty() {
                // Compartment model (matches a normal transition + `-> pop$`).
                // The runtime has no `_transition(name, …)` method — only
                // `__transition(compartment)`. See FRAMEC_BUGS Issue #42.
                format!("{}\n{}__compartment = self.__prepareEnter(\"{}\", [], [])\n{}self.__transition(__compartment)", push_code, indent_str, target, indent_str)
            } else {
                push_code
            }
        }
        TargetLanguage::GDScript => {
            let push_code = format!("{}self._state_stack.append(self.__compartment)", indent_str);
            if !target.is_empty() {
                // Compartment model (matches a normal transition + `-> pop$`).
                // The runtime has no `_transition(name, …)` func — only
                // `__transition(next_compartment)`. See FRAMEC_BUGS Issue #42.
                format!("{}\n{}var __compartment = self.__prepareEnter(\"{}\", [], [])\n{}self.__transition(__compartment)", push_code, indent_str, target, indent_str)
            } else {
                push_code
            }
        }
        TargetLanguage::TypeScript | TargetLanguage::JavaScript => {
            let push_code = format!("{}this._state_stack.push(this.__compartment);", indent_str);
            if !target.is_empty() {
                // Transition the same way a normal transition does (compartment
                // model: __prepareEnter + __transition + return). The JS/TS
                // runtime has no `_transition(name, …)` method — only
                // `__transition(compartment)` — so the old form threw at
                // runtime. See FRAMEC_BUGS Issue #42.
                format!("{}\n{}const __compartment = this.__prepareEnter(\"{}\", [], []);\n{}this.__transition(__compartment);", push_code, indent_str, target, indent_str)
            } else {
                push_code
            }
        }
        TargetLanguage::Dart => {
            let push_code = format!("{}this._state_stack.add(this.__compartment);", indent_str);
            if !target.is_empty() {
                format!(
                    "{}\n{}this.__transition({}Compartment(\"{}\"));",
                    push_code, indent_str, ctx.system_name, target
                )
            } else {
                push_code
            }
        }
        TargetLanguage::Rust => {
            if !target.is_empty() {
                super::super::rust_system::rust_push_transition(&indent_str, ctx, &target)
            } else {
                super::super::rust_system::rust_bare_push(&indent_str)
            }
        }
        TargetLanguage::C => {
            // C: save reference via ref count increment. The stack
            // holds a ref'd pointer. The kernel's _unref on
            // transition won't free it while the stack holds a ref.
            let push_code = format!(
                "{}{}_FrameVec_push(self->_state_stack, {}_Compartment_ref(self->__compartment));",
                indent_str, ctx.system_name, ctx.system_name
            );
            if !target.is_empty() {
                format!(
                    "{}\n{}{}_transition(self, {}_Compartment_new(\"{}\"));",
                    push_code, indent_str, ctx.system_name, ctx.system_name, target
                )
            } else {
                push_code
            }
        }
        TargetLanguage::Cpp => {
            // C++: shared_ptr reference save (ref count increment).
            let push_code = format!("{}_state_stack.push_back(__compartment);", indent_str);
            if !target.is_empty() {
                format!(
                    "{}\n{}__transition(std::make_shared<{}Compartment>(\"{}\"));",
                    push_code, indent_str, ctx.system_name, target
                )
            } else {
                push_code
            }
        }
        TargetLanguage::Java => {
            let push_code = format!("{}_state_stack.add(__compartment);", indent_str);
            if !target.is_empty() {
                format!(
                    "{}\n{}__transition(new {}Compartment(\"{}\"));",
                    push_code, indent_str, ctx.system_name, target
                )
            } else {
                push_code
            }
        }
        TargetLanguage::Kotlin => {
            let push_code = format!("{}_state_stack.add(__compartment)", indent_str);
            if !target.is_empty() {
                format!(
                    "{}\n{}__transition({}Compartment(\"{}\"))",
                    push_code, indent_str, ctx.system_name, target
                )
            } else {
                push_code
            }
        }
        TargetLanguage::Swift => {
            let push_code = format!("{}_state_stack.append(__compartment)", indent_str);
            if !target.is_empty() {
                format!(
                    "{}\n{}__transition({}Compartment(state: \"{}\"))",
                    push_code, indent_str, ctx.system_name, target
                )
            } else {
                push_code
            }
        }
        TargetLanguage::Go => {
            let push_code = format!(
                "{}s._state_stack = append(s._state_stack, s.__compartment)",
                indent_str
            );
            if !target.is_empty() {
                format!(
                    "{}\n{}s.__transition(new{}Compartment(\"{}\"))",
                    push_code, indent_str, ctx.system_name, target
                )
            } else {
                push_code
            }
        }
        TargetLanguage::CSharp => {
            let push_code = format!("{}_state_stack.Add(__compartment);", indent_str);
            if !target.is_empty() {
                format!(
                    "{}\n{}__transition(new {}Compartment(\"{}\"));",
                    push_code, indent_str, ctx.system_name, target
                )
            } else {
                push_code
            }
        }
        TargetLanguage::Php => {
            let push_code = format!(
                "{}$this->_state_stack[] = $this->__compartment;",
                indent_str
            );
            if !target.is_empty() {
                format!(
                    "{}\n{}$this->__transition(new {}Compartment(\"{}\"));",
                    push_code, indent_str, ctx.system_name, target
                )
            } else {
                push_code
            }
        }
        TargetLanguage::Ruby => {
            let push_code = format!("{}@_state_stack.push(@__compartment)", indent_str);
            if !target.is_empty() {
                format!(
                    "{}\n{}__transition({}Compartment.new(\"{}\"))",
                    push_code, indent_str, ctx.system_name, target
                )
            } else {
                push_code
            }
        }
        TargetLanguage::Lua => {
            let push_code = format!(
                "{}self._state_stack[#self._state_stack + 1] = self.__compartment",
                indent_str
            );
            if !target.is_empty() {
                format!(
                    "{}\n{}self:__transition({}Compartment.new(\"{}\"))",
                    push_code, indent_str, ctx.system_name, target
                )
            } else {
                push_code
            }
        }
        TargetLanguage::Graphviz => unreachable!(),
    };
    // A bare `push$` (empty target) is not terminal — no return. A
    // push-transition exits the handler like any other transition.
    let terminator = if target.is_empty() {
        ""
    } else {
        super::utility::transition_terminator(lang)
    };
    (body, terminator)
}

/// Expand a standalone `pop$` (discard the stack top) into `(body, terminator)`.
/// This form is NOT a transition — it never exits the handler — so the
/// terminator is always `""`. (Transitioning to the popped state is `-> pop$`,
/// handled by `generate_pop_transition`.)
pub(super) fn expand_stack_pop(
    segment_text: &str,
    indent: usize,
    lang: TargetLanguage,
    ctx: &HandlerContext,
    metadata: &SegmentMetadata,
) -> (String, &'static str) {
    let indent_str = " ".repeat(indent);

    // Standalone pop$ — pop the top of the stack and discard it.
    // No transition. For transitioning to the popped state, use -> pop$.
    let body = match lang {
        TargetLanguage::Python3 => format!("{}self._state_stack.pop()", indent_str),
        TargetLanguage::GDScript => format!("{}self._state_stack.pop_back()", indent_str),
        TargetLanguage::TypeScript => format!("{}this._state_stack.pop();", indent_str),
        TargetLanguage::JavaScript => format!("{}this._state_stack.pop();", indent_str),
        TargetLanguage::Dart => format!("{}this._state_stack.removeLast();", indent_str),
        TargetLanguage::Rust => super::super::rust_system::rust_bare_pop(&indent_str),
        TargetLanguage::C => format!(
            "{}{}_FrameVec_pop(self->_state_stack);",
            indent_str, ctx.system_name
        ),
        TargetLanguage::Cpp => format!("{}_state_stack.pop_back();", indent_str),
        TargetLanguage::Java => format!(
            "{}_state_stack.remove(_state_stack.size() - 1);",
            indent_str
        ),
        TargetLanguage::Kotlin => {
            format!("{}_state_stack.removeAt(_state_stack.size - 1)", indent_str)
        }
        TargetLanguage::Swift => format!("{}_state_stack.removeLast()", indent_str),
        TargetLanguage::CSharp => format!(
            "{}_state_stack.RemoveAt(_state_stack.Count - 1);",
            indent_str
        ),
        TargetLanguage::Go => format!(
            "{}s._state_stack = s._state_stack[:len(s._state_stack)-1]",
            indent_str
        ),
        TargetLanguage::Php => format!("{}array_pop($this->_state_stack);", indent_str),
        TargetLanguage::Ruby => format!("{}@_state_stack.pop", indent_str),
        TargetLanguage::Lua => format!("{}table.remove(self._state_stack)", indent_str),
        TargetLanguage::Graphviz => unreachable!(),
    };
    // Standalone `pop$` is not terminal — it never exits the handler.
    (body, "")
}

/// The per-language "save the current compartment onto the state stack" line.
///
/// Extracted (#211) so an arg-bearing `push$ -> $S(7)` can emit the push and then
/// DELEGATE the transition to the transition expander, instead of this file
/// re-implementing transition emission per target — which is how the args got lost.
fn push_compartment_line(indent_str: &str, lang: TargetLanguage, ctx: &HandlerContext) -> String {
    match lang {
        TargetLanguage::Rust => super::super::rust_system::rust_bare_push(indent_str),
        TargetLanguage::C => format!(
            "{}{}_FrameVec_push(self->_state_stack, {}_Compartment_ref(self->__compartment));",
            indent_str, ctx.system_name, ctx.system_name
        ),
        TargetLanguage::Cpp => format!("{}_state_stack.push_back(__compartment);", indent_str),
        TargetLanguage::Go => format!(
            "{}s._state_stack = append(s._state_stack, s.__compartment)",
            indent_str
        ),
        TargetLanguage::Php => format!(
            "{}$this->_state_stack[] = $this->__compartment;",
            indent_str
        ),
        TargetLanguage::Lua => format!(
            "{}self._state_stack[#self._state_stack + 1] = self.__compartment",
            indent_str
        ),
        TargetLanguage::Python3 => {
            format!("{}self._state_stack.append(self.__compartment)", indent_str)
        }
        TargetLanguage::GDScript => {
            format!("{}self._state_stack.append(self.__compartment)", indent_str)
        }
        TargetLanguage::TypeScript | TargetLanguage::JavaScript => {
            format!("{}this._state_stack.push(this.__compartment);", indent_str)
        }
        TargetLanguage::Dart => format!("{}this._state_stack.add(this.__compartment);", indent_str),
        TargetLanguage::Java => format!("{}_state_stack.add(__compartment);", indent_str),
        TargetLanguage::Kotlin => format!("{}_state_stack.add(__compartment)", indent_str),
        TargetLanguage::Swift => format!("{}_state_stack.append(__compartment)", indent_str),
        TargetLanguage::CSharp => format!("{}_state_stack.Add(__compartment);", indent_str),
        TargetLanguage::Ruby => format!("{}@_state_stack.push(@__compartment)", indent_str),
        // Exhaustive by intent: GraphViz emits a diagram, not a runtime.
        TargetLanguage::Graphviz => String::new(),
    }
}
