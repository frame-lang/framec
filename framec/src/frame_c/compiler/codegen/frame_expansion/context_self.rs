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

use super::super::codegen_utils::{capitalize_first, to_snake_case, HandlerContext};
use super::expand_expression;
use super::utility::strip_outer_parens;
use crate::frame_c::compiler::native_region_scanner::{RegionSpan, SegmentMetadata};
use crate::frame_c::visitors::TargetLanguage;

/// #159 round 3 — the unique defined-system name appearing as an identifier
/// token (word-boundary delimited) in a declared type string. `None` when no
/// system name appears, or when two DIFFERENT system names do (ambiguous).
fn unique_system_token(
    type_str: &str,
    systems: &std::collections::HashSet<String>,
) -> Option<String> {
    let is_word = |b: u8| b.is_ascii_alphanumeric() || b == b'_';
    let bytes = type_str.as_bytes();
    let mut hit: Option<&str> = None;
    for sys in systems {
        let mut from = 0usize;
        while let Some(off) = type_str[from..].find(sys.as_str()) {
            let start = from + off;
            let end = start + sys.len();
            let left_ok = start == 0 || !is_word(bytes[start - 1]);
            let right_ok = end >= bytes.len() || !is_word(bytes[end]);
            if left_ok && right_ok {
                match hit {
                    Some(prev) if prev != sys.as_str() => return None, // ambiguous
                    _ => hit = Some(sys.as_str()),
                }
                break;
            }
            from = end;
        }
    }
    hit.map(|s| s.to_string())
}

/// #159 round 3 (C++): whether the resolved element is accessed through a
/// pointer — `Counter*` / `shared_ptr<Counter>` / `unique_ptr<Counter>` — vs
/// held by value (`std::vector<Counter>`), read structurally off the declared
/// type around the system-name token.
fn cpp_element_is_pointer(type_str: &str, sys: &str) -> bool {
    let is_word = |b: u8| b.is_ascii_alphanumeric() || b == b'_';
    let bytes = type_str.as_bytes();
    let mut from = 0usize;
    while let Some(off) = type_str[from..].find(sys) {
        let start = from + off;
        let end = start + sys.len();
        let left_ok = start == 0 || !is_word(bytes[start - 1]);
        let right_ok = end >= bytes.len() || !is_word(bytes[end]);
        if left_ok && right_ok {
            // `Counter *` / `Counter*` after the token → raw pointer.
            let after = type_str[end..].trim_start();
            if after.starts_with('*') {
                return true;
            }
            // `shared_ptr<Counter` / `unique_ptr<Counter` before → smart ptr.
            let before = &type_str[..start];
            let b = before.trim_end();
            if b.ends_with("shared_ptr<") || b.ends_with("unique_ptr<") {
                return true;
            }
            return false;
        }
        from = end;
    }
    false
}

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
        // #175: a domain field named after a Swift keyword must be
        // backtick-escaped at the access site to match its escaped declaration.
        let field = if matches!(lang, TargetLanguage::Swift) {
            super::super::codegen_utils::swift_escape_ident(field)
        } else {
            std::borrow::Cow::Borrowed(field.as_str())
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
    let (field, method, raw_args, index) = if let SegmentMetadata::SelfFieldCall {
        field,
        method,
        args,
        index,
    } = metadata
    {
        (
            field.as_str(),
            method.as_str(),
            args.as_str(),
            index.as_deref(),
        )
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

    // #159: the indexed form `@@:self.field[i].method(args)` splices the
    // bracket group after the field on every target; `idx` is `""` for the
    // plain field call. The index expression may itself contain Frame syntax
    // (`@@:self.list[@@:self.cn]`), so expand it like the args.
    let idx: String = match index {
        Some(raw) if raw.len() >= 2 && raw.starts_with('[') && raw.ends_with(']') => {
            let inner = &raw[1..raw.len() - 1];
            if inner.trim().is_empty() {
                raw.to_string()
            } else {
                format!("[{}]", expand_expression(inner, lang, ctx))
            }
        }
        Some(raw) => raw.to_string(),
        None => String::new(),
    };

    // Embed = the field's declared type is itself a defined system. The
    // declared spelling may be pointer-qualified — the C guide's idiomatic
    // cross-system field is `inner: Inner*` (#73) — so strip trailing `*`s
    // to get the base system name for the check (and for C's free-function
    // family / Erlang's module name below).
    //
    // #159 resolution ladder for the INDEXED form, whose element type is
    // routinely hidden behind a native container typedef
    // (`counters: CounterArr`), which framec — type-ignorant by design —
    // cannot see through:
    //   1. declared type minus trailing `[..]` group and `*`s ∈ systems, or
    //   2. the UNIQUE system whose Frame-declared interface has `method`
    //      (arcanum knowledge, not native-type parsing). Ambiguous or
    //      unknown → native passthrough, same as a scalar field.
    // Rule 2 applies ONLY to the indexed form: on the plain form a scalar
    // field's native method call must stay native even if its name collides
    // with some system's interface method.
    let field_type = ctx.domain_field_types.get(field);
    let embed_base_owned: Option<String> = {
        // Plain form: exactly the pre-#159 rule (type minus trailing `*`s).
        let plain = field_type
            .map(|t| {
                t.trim()
                    .trim_start_matches('*')
                    .trim_end_matches('*')
                    .trim()
                    .to_string()
            })
            .filter(|base| ctx.defined_systems.contains(base));
        if plain.is_some() {
            plain
        } else if index.is_some() {
            // Indexed only — an unindexed call on an array-typed field is a
            // native container-method call (`list.push(x)`) and must never
            // resolve to a system.
            //
            // TOKEN RULE (#159 round 3): the element system is the UNIQUE
            // defined-system name appearing as an identifier token in the
            // declared type string. One structural rule covers every visible
            // spelling — `Counter*[4]`, `[]*Ghost`, `std::vector<Counter*>`,
            // `Rc<RefCell<Counter>>`, Lua's informational `Counter[]` — while
            // a native typedef (`CounterArr`) still resolves to nothing
            // (word boundary), which is correct: it is physically opaque to
            // a type-ignorant compiler.
            let elem = field_type.and_then(|t| unique_system_token(t, &ctx.defined_systems));
            if elem.is_some() {
                elem
            } else {
                crate::frame_c::compiler::codegen::interface_gen::unique_system_with_interface_method(
                    method,
                    &ctx.system_name,
                )
                .filter(|sys| ctx.defined_systems.contains(sys))
            }
        } else {
            None
        }
    };
    let embed_base: Option<&str> = embed_base_owned.as_deref();
    let is_embed = embed_base.is_some();
    // Erlang lowers EVERY field call as a cross-system module call (it has no
    // method-on-value syntax), keyed off the raw declared type base — no
    // defined_systems membership required (matches the pre-#159 behavior; the
    // type may be cross-file). Indexed fields strip the `[..]` group first,
    // falling back to the resolved system when the typedef hides the element.
    let erlang_base_owned: Option<String> = field_type
        .map(|t| {
            let t = t.trim();
            let no_arr = if index.is_some() {
                match t.find('[') {
                    Some(b) => t[..b].trim_end(),
                    None => t,
                }
            } else {
                t
            };
            no_arr.trim_end_matches('*').trim_end().to_string()
        })
        .filter(|b| !b.is_empty())
        .or_else(|| embed_base_owned.clone());

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
                    format!("{sys}_{method}(self->{field}{idx})")
                } else {
                    format!("{sys}_{method}(self->{field}{idx}, {inner})")
                }
            } else {
                format!("self->{field}{idx}.{method}{args}")
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
            if let Some(base) = erlang_base_owned.as_deref() {
                let module = to_snake_case(base);
                let inner = strip_outer_parens(&args);
                if inner.trim().is_empty() {
                    format!("{module}:{method}(self.{field}{idx})")
                } else {
                    format!("{module}:{method}(self.{field}{idx}, {inner})")
                }
            } else {
                format!("self.{field}{idx}.{method}{args}")
            }
        }
        // C++: embed fields are `shared_ptr` (deref with `->`); scalar fields are
        // values (`.`). This is the one target where the field type changes the
        // method-access operator.
        TargetLanguage::Cpp => {
            // Indexed elements read their pointer-ness structurally off the
            // declared type (`vector<Counter*>` → `->`, `vector<Counter>` →
            // `.`); a plain embed field is a framec-emitted shared_ptr (`->`).
            let mop = if is_embed {
                if index.is_some() {
                    // When the type names the system (token rule), read the
                    // pointer-ness off it. When resolution came from the
                    // method-uniqueness fallback (type hidden behind a
                    // typedef), keep the historical `->` assumption — the
                    // canonical container shapes are pointer elements.
                    match embed_base.zip(field_type) {
                        Some((sys, t)) if t.contains(sys) => {
                            if cpp_element_is_pointer(t, sys) {
                                "->"
                            } else {
                                "."
                            }
                        }
                        _ => "->",
                    }
                } else {
                    "->"
                }
            } else {
                "."
            };
            format!("this->{field}{idx}{mop}{method}{args}")
        }
        // PHP: every object method call uses `->`.
        TargetLanguage::Php => format!("$this->{field}{idx}->{method}{args}"),
        // Go exports interface methods by capitalizing the first letter
        // (`tick` → `Tick`). A cross-system (embed) call must use that same
        // exported spelling, or it references an undefined method (#112). A
        // non-embed scalar-field call targets a native Go method whose name
        // framec does not control, so it stays verbatim.
        TargetLanguage::Go => {
            if is_embed {
                format!("s.{field}{idx}.{}{args}", capitalize_first(method))
            } else {
                format!("s.{field}{idx}.{method}{args}")
            }
        }
        TargetLanguage::Python3
        | TargetLanguage::GDScript
        | TargetLanguage::Ruby
        | TargetLanguage::Rust => format!("self.{field}{idx}.{method}{args}"),
        // Swift: backtick-escape a field/method name that collides with a Swift
        // keyword (#175) — e.g. a composed system's canonical `init(...)`
        // interface method → `self.kid.`init`(5)`. The escaper is a no-op for
        // non-keyword names.
        TargetLanguage::Swift => {
            let field = super::super::codegen_utils::swift_escape_ident(field);
            let method = super::super::codegen_utils::swift_escape_ident(method);
            format!("self.{field}{idx}.{method}{args}")
        }
        // Lua method calls use `:` (implicit self). A cross-system (embed) call
        // must use `:`, or `self` is not passed and the first real argument
        // shifts into it (#120 — the Lua analog of Go #112). A non-embed scalar
        // field's native method stays `.` (framec does not control its name).
        TargetLanguage::Lua => {
            if is_embed {
                format!("self.{field}{idx}:{method}{args}")
            } else {
                format!("self.{field}{idx}.{method}{args}")
            }
        }
        TargetLanguage::TypeScript
        | TargetLanguage::JavaScript
        | TargetLanguage::Dart
        | TargetLanguage::Java
        | TargetLanguage::Kotlin
        | TargetLanguage::CSharp => format!("this.{field}{idx}.{method}{args}"),
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
            // Rust's borrow checker rejects `self.foo(self.bar(x))` because both
            // calls take `&mut self` at once. Hoist EACH nested self-call arg
            // into its own sequential `let` (two separate borrows, not
            // simultaneous), then call with the temps:
            //   { let __rs_tmp_arg0 = self.bar(x); let __rs_tmp_arg1 = self.baz(y);
            //     self.foo(__rs_tmp_arg0, __rs_tmp_arg1) }
            //
            // #150: string/comment- and depth-aware. The old form wrapped the
            // *whole* arg string in one binding, so two self-call args produced
            // `let t = self.a(x), self.b(y);` (invalid Rust), and a `self.`
            // inside a string-literal arg triggered a spurious hoist.
            // `contains_receiver_call` ignores literals/comments and bare field
            // accesses; `split_top_level_args` splits only at depth-0 commas.
            use crate::frame_c::compiler::codegen::codegen_utils::{
                contains_receiver_call, split_top_level_args,
            };
            let inner = strip_outer_parens(args_with_parens);
            if !inner.trim().is_empty()
                && contains_receiver_call(inner, TargetLanguage::Rust, "self")
            {
                let args = split_top_level_args(inner, TargetLanguage::Rust);
                let mut prelude = String::new();
                let mut call_args = Vec::with_capacity(args.len());
                for (n, arg) in args.iter().enumerate() {
                    if contains_receiver_call(arg, TargetLanguage::Rust, "self") {
                        let tmp = format!("__rs_tmp_arg{n}");
                        prelude.push_str(&format!("let {tmp} = {arg}; "));
                        call_args.push(tmp);
                    } else {
                        call_args.push(arg.clone());
                    }
                }
                format!(
                    "{{ {}self.{}({}) }}",
                    prelude,
                    method_name,
                    call_args.join(", ")
                )
            } else {
                format!("self.{}{}", method_name, args_with_parens)
            }
        }
        TargetLanguage::Swift => {
            // Backtick-escape a self-call to an interface method whose name is a
            // Swift keyword (#175), e.g. `@@:self.init(x)` → `self.`init`(x)`.
            let method_name = super::super::codegen_utils::swift_escape_ident(method_name);
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
            // Interface methods are exported (PascalCase); actions are private
            // helpers and stay lowercase. Capitalize only an interface self-call,
            // not an `@@:self.<action>()` call, or it would reference an
            // undefined exported method (#115).
            let go_method = if ctx.actions.contains(method_name) {
                method_name.to_string()
            } else {
                format!("{}{}", method_name[..1].to_uppercase(), &method_name[1..])
            };
            format!("s.{}{}", go_method, args_with_parens)
        }
        TargetLanguage::Php => format!("$this->{}{}", method_name, args_with_parens),
        TargetLanguage::Ruby => format!("self.{}{}", method_name, args_with_parens),
        TargetLanguage::Lua => format!("self:{}{}", method_name, args_with_parens),
        TargetLanguage::Erlang => {
            // Emit the `@@:self.method(args)` form *with the marker
            // preserved* and let the Erlang handler post-pass
            // (erlang_system.rs::erlang_rewrite_native_classified_full)
            // recognize the marked pattern as an `ActionCall` /
            // `InterfaceCall` and rewrite it to `action(Data, args)` /
            // `{DataN, Result} = frame_dispatch__(method, [args],
            // DataPrev)`. That pass threads NewData forward through the
            // rest of the handler body via `data_gen`/`data_var` — so
            // `self.field` reads and `-> $State` transitions after a
            // @@:self call correctly see the state changes the called
            // handler made.
            //
            // Core Frame rule: framec translates only Frame syntax. The
            // `@@:self.` marker is what makes this call *Frame-derived*
            // and therefore translatable. A *bare* native `self.X(...)`
            // carries no marker, so the classifier leaves it verbatim
            // and `erlc` rejects it on its own line (Erlang has no
            // `self` value) — the correct, contextual native behavior.
            format!("@@:self.{}{}", method_name, args_with_parens)
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
