//! `@@:self` / `@@:self.method(...)` self-reference / re-entrant
//! dispatch expansion.
//!
//! Two arms:
//!
//! - `expand_context_self` — bare `@@:self`. Emits the per-target
//!   self / instance pointer (`self` in most languages, `this` in
//!   the C-family / TS, `&mut self` for Rust, etc.).
//! - `expand_context_self_call` — `@@:self.method(args)`.
//!   Re-entrant call back into the running system's dispatch.
//!   The transition guard `if _transitioned then return` is
//!   emitted separately by `emit_handler_body_via_statements` so
//!   it lands at a statement boundary.

use super::super::codegen_utils::{to_snake_case, HandlerContext};
use super::expand_expression;
use super::utility::strip_outer_parens;
use crate::frame_c::compiler::native_region_scanner::{RegionSpan, SegmentMetadata};
use crate::frame_c::visitors::TargetLanguage;

pub(super) fn expand_context_self(
    _body_bytes: &[u8],
    _span: &RegionSpan,
    _indent: usize,
    lang: TargetLanguage,
    _ctx: &HandlerContext,
    metadata: &SegmentMetadata,
) -> String {
    // Per-target self/instance receiver. Bare `@@:self` is rejected by the
    // validator (E603); this value is the prefix for `@@:self.<field>`
    // (RFC-0046) and the existing self-call path.
    let receiver = match lang {
        TargetLanguage::Python3
        | TargetLanguage::GDScript
        | TargetLanguage::Ruby
        | TargetLanguage::Lua
        | TargetLanguage::Swift => "self",
        TargetLanguage::TypeScript
        | TargetLanguage::JavaScript
        | TargetLanguage::Java
        | TargetLanguage::Kotlin
        | TargetLanguage::CSharp
        | TargetLanguage::Dart => "this",
        TargetLanguage::Cpp => "this",
        TargetLanguage::C => "self",
        TargetLanguage::Go => "s",
        TargetLanguage::Php => "$this",
        TargetLanguage::Rust => super::super::rust_system::rust_self_ref(),
        TargetLanguage::Erlang => "self",
        TargetLanguage::Graphviz => unreachable!(),
    };

    // `@@:self.<field>` (RFC-0046): portable domain-field reference. Lower to
    // the target's native member access. Pointer-receiver targets use `->`
    // (C++ `this->`, PHP `$this->`, C `self->`); the rest use `.`. Erlang
    // emits `self.field`, which its domain post-pass threads into the `#data`
    // record (read → `Data#data.field`, write → record update) — the same path
    // a native `self.field` already takes. Ruby uses `attr_accessor`, so
    // `self.field` works as both lvalue and rvalue.
    if let SegmentMetadata::SelfField { field } = metadata {
        let sep = match lang {
            TargetLanguage::Cpp | TargetLanguage::Php | TargetLanguage::C => "->",
            _ => ".",
        };
        format!("{receiver}{sep}{field}")
    } else {
        // Bare `@@:self` — rejected by the validator before codegen; emit the
        // receiver so nothing leaks if a caller skips validation.
        receiver.to_string()
    }
}

/// `@@:self.field.method(args)` (RFC-0046) — a call through a self field.
/// If `field` is an embedded system (its declared type is a defined system),
/// this is a cross-system interface call; framec emits the per-target access +
/// call punctuation matching the field's storage. Otherwise it is a native
/// method call on a scalar field's value. No caller-side transition guard is
/// emitted (the call enters a *different* system's dispatch, if any).
pub(super) fn expand_context_self_field_call(
    _body_bytes: &[u8],
    _span: &RegionSpan,
    _indent: usize,
    lang: TargetLanguage,
    ctx: &HandlerContext,
    metadata: &SegmentMetadata,
) -> String {
    let (field, method, raw_args) = if let SegmentMetadata::SelfFieldCall {
        field,
        method,
        args,
    } = metadata
    {
        (field.as_str(), method.as_str(), args.as_str())
    } else {
        return String::new();
    };

    // Expand any Frame syntax nested in the args (e.g.
    // `@@:self.ship.fire(@@:self.power)`).
    let args = if raw_args.len() >= 2 && raw_args.starts_with('(') && raw_args.ends_with(')') {
        let inner = strip_outer_parens(raw_args);
        if inner.is_empty() {
            raw_args.to_string()
        } else {
            format!("({})", expand_expression(inner, lang, ctx))
        }
    } else {
        raw_args.to_string()
    };

    // Embed = the field's declared type is itself a defined system. The
    // declared spelling may be pointer-qualified — the C guide's idiomatic
    // cross-system field is `inner: Inner*` (#73) — so strip trailing `*`s
    // to get the base system name for the check (and for C's free-function
    // family / Erlang's module name below).
    let field_type = ctx.domain_field_types.get(field);
    let embed_base: Option<&str> = field_type.map(|t| t.trim().trim_end_matches('*').trim_end());
    let is_embed = embed_base.is_some_and(|base| ctx.defined_systems.contains(base));

    match lang {
        // C: a struct has no methods, so an embed call is a cross-system
        // free-function call `Sys_method(self->field, args)` — emitted directly
        // from the segment (RFC-0046 d-cross; replaces the textual
        // `rewrite_c_cross_system_calls` post-pass). A scalar-field method stays
        // native (`self->field.method(args)`).
        TargetLanguage::C => {
            if is_embed {
                let sys = embed_base.unwrap_or("");
                let inner = strip_outer_parens(&args);
                if inner.trim().is_empty() {
                    format!("{sys}_{method}(self->{field})")
                } else {
                    format!("{sys}_{method}(self->{field}, {inner})")
                }
            } else {
                format!("self->{field}.{method}{args}")
            }
        }
        // Erlang: an embed field holds a Pid, so a cross-system call is a
        // module-qualified dispatch `module:method(self.field, args)` — emitted
        // directly from the segment (RFC-0046 d-cross; replaces the textual
        // cross-system rewriter). The trailing `self.field` is turned into
        // `Data#data.field` (the Pid read) by the existing domain post-pass.
        // (Erlang has no method-call-on-value syntax, so `@@:self.field.method()`
        // is always a cross-system call; the module name is the field's system
        // type, snake-cased — matching the field's `@@System()` initializer.)
        TargetLanguage::Erlang => {
            if let Some(base) = embed_base {
                let module = to_snake_case(base);
                let inner = strip_outer_parens(&args);
                if inner.trim().is_empty() {
                    format!("{module}:{method}(self.{field})")
                } else {
                    format!("{module}:{method}(self.{field}, {inner})")
                }
            } else {
                format!("self.{field}.{method}{args}")
            }
        }
        // C++: embed fields are `shared_ptr` (deref with `->`); scalar fields are
        // values (`.`). This is the one target where the field type changes the
        // method-access operator.
        TargetLanguage::Cpp => {
            let mop = if is_embed { "->" } else { "." };
            format!("this->{field}{mop}{method}{args}")
        }
        // PHP: every object method call uses `->`.
        TargetLanguage::Php => format!("$this->{field}->{method}{args}"),
        TargetLanguage::Go => format!("s.{field}.{method}{args}"),
        TargetLanguage::Python3
        | TargetLanguage::GDScript
        | TargetLanguage::Ruby
        | TargetLanguage::Lua
        | TargetLanguage::Swift
        | TargetLanguage::Rust => format!("self.{field}.{method}{args}"),
        TargetLanguage::TypeScript
        | TargetLanguage::JavaScript
        | TargetLanguage::Dart
        | TargetLanguage::Java
        | TargetLanguage::Kotlin
        | TargetLanguage::CSharp => format!("this.{field}.{method}{args}"),
        TargetLanguage::Graphviz => unreachable!(),
    }
}

pub(super) fn expand_context_self_call(
    body_bytes: &[u8],
    span: &RegionSpan,
    indent: usize,
    lang: TargetLanguage,
    ctx: &HandlerContext,
    metadata: &SegmentMetadata,
) -> String {
    let segment_text = String::from_utf8_lossy(&body_bytes[span.start..span.end]);
    let indent_str = " ".repeat(indent);

    // @@:self.method(args) — reentrant interface call with transition guard
    // Extract method name and args from segment text: @@:self.method(args)
    let trimmed = segment_text.trim();
    let (method_name, raw_args_with_parens) =
        if let SegmentMetadata::SelfCall { method, args } = metadata {
            (method.as_str(), args.as_str())
        } else {
            let after_self = trimmed.strip_prefix("@@:self.").unwrap_or(trimmed);
            let paren_pos = after_self.find('(').unwrap_or(after_self.len());
            (&after_self[..paren_pos], &after_self[paren_pos..])
        };
    // Recursively expand Frame syntax nested inside the args —
    // e.g. `@@:self.foo(@@:return)`, `@@:self.foo(@@:params.x)`,
    // `@@:self.foo(self.op())`, etc. Without this the inner
    // segment would leak verbatim into target source and fail
    // to parse (e.g. literal `@@:return` in Python output).
    let expanded_args = if raw_args_with_parens.len() >= 2
        && raw_args_with_parens.starts_with('(')
        && raw_args_with_parens.ends_with(')')
    {
        let inner = strip_outer_parens(raw_args_with_parens);
        if inner.is_empty() {
            raw_args_with_parens.to_string()
        } else {
            format!("({})", expand_expression(inner, lang, ctx))
        }
    } else {
        raw_args_with_parens.to_string()
    };
    let args_with_parens = expanded_args.as_str();

    // Generate the native self-call
    let call_expr = match lang {
        TargetLanguage::Python3 | TargetLanguage::GDScript => {
            format!("self.{}{}", method_name, args_with_parens)
        }
        TargetLanguage::TypeScript | TargetLanguage::JavaScript | TargetLanguage::Dart => {
            format!("this.{}{}", method_name, args_with_parens)
        }
        TargetLanguage::Rust => {
            // Rust's borrow checker rejects `self.foo(self.bar(x))`
            // because both calls take `&mut self` at the same time.
            // When the already-expanded args contain another
            // `self.<method>(` pattern, hoist the inner call into
            // a let-binding inside a block expression:
            //   { let __rs_tmpN = self.bar(x); self.foo(__rs_tmpN) }
            // Sequential `let` bindings in a block are two
            // separate borrows — not simultaneous — so the
            // checker accepts.
            if args_with_parens.contains("self.") {
                let inner = strip_outer_parens(args_with_parens);
                format!(
                    "{{ let __rs_tmp_arg = {}; self.{}(__rs_tmp_arg) }}",
                    inner, method_name
                )
            } else {
                format!("self.{}{}", method_name, args_with_parens)
            }
        }
        TargetLanguage::Swift => {
            format!("self.{}{}", method_name, args_with_parens)
        }
        TargetLanguage::Cpp => format!("this->{}{}", method_name, args_with_parens),
        TargetLanguage::C => {
            if args_with_parens == "()" {
                format!("{}_{}(self)", ctx.system_name, method_name)
            } else {
                let inner_args = strip_outer_parens(args_with_parens);
                format!("{}_{}(self, {})", ctx.system_name, method_name, inner_args)
            }
        }
        TargetLanguage::Java | TargetLanguage::Kotlin | TargetLanguage::CSharp => {
            format!("this.{}{}", method_name, args_with_parens)
        }
        TargetLanguage::Go => {
            let go_method = format!("{}{}", method_name[..1].to_uppercase(), &method_name[1..]);
            format!("s.{}{}", go_method, args_with_parens)
        }
        TargetLanguage::Php => format!("$this->{}{}", method_name, args_with_parens),
        TargetLanguage::Ruby => format!("self.{}{}", method_name, args_with_parens),
        TargetLanguage::Lua => format!("self:{}{}", method_name, args_with_parens),
        TargetLanguage::Erlang => {
            // Emit bare `self.method(args)` and let the Erlang
            // handler post-pass (erlang_system.rs::
            // erlang_rewrite_native_classified_full) recognize the
            // pattern as an `InterfaceCall` and rewrite it to
            // `{DataN, Result} = frame_dispatch__(method, [args],
            // DataPrev)`. That pass threads NewData forward
            // through the rest of the handler body via
            // `data_gen`/`data_var` — so `self.field` reads and
            // `-> $State` transitions after a @@:self call
            // correctly see the state changes the called
            // handler made.
            format!("self.{}{}", method_name, args_with_parens)
        }
        TargetLanguage::Graphviz => unreachable!(),
    };

    // @@:self.method() — check if standalone (only whitespace before @@:
    // in the source) or inline (preceded by native code like `x = `).
    // The scanner trims trailing whitespace from native text for
    // standalone constructs, so we must provide indent_str. For
    // inline, native text provides the indent.
    //
    // We detect this from the segment_text position: if the segment
    // starts at a position where the preceding byte is whitespace or
    // newline, it's standalone. The scanner always sets indent > 0
    // for self-calls (line's leading whitespace for the guard), so
    // we can't use indent == 0 as the inline signal.
    //
    // Instead, check if the raw output ends with whitespace (inline
    // context: native text like "baseline = " precedes us) or with
    // a newline (standalone: previous line ended, we start fresh).
    //
    // Actually, the simplest correct approach: the expansion is
    // always just the call expression. The orchestrator adds the
    // guard. For standalone, the scanner trimmed the whitespace so
    // indent_str fills the gap. For inline, the scanner kept the
    // native text. In BOTH cases, indent_str is correct:
    //   standalone: trimmed ws (16 sp) + indent_str (16 sp) call = 16 sp call ✓
    //   inline: native "baseline = " + indent_str (16 sp) call = broken!
    //
    // So we DO need to distinguish. Use the preceding native text:
    // if it was trimmed (standalone), the segment immediately follows
    // a newline in the output. If not trimmed (inline), it follows
    // non-newline content. But we don't have access to `out` here.
    //
    // Cleanest: just return call_expr. The standalone case needs
    // indent_str, which the orchestrator can add based on indent > 0
    // and whether the expansion doesn't already start with whitespace.
    call_expr
}
