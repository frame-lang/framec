//! Handler body splicing + per-target `@@:return` typed read.
//!
//! Three closely-coupled helpers, all called from the per-state
//! handler emission path:
//!
//! - `resolve_state_arg_key(i, target_state, ctx)` — resolves
//!   the storage key for a positional state-arg in a transition.
//!   Returns the declared param name from the target state's
//!   signature when available, otherwise the bare index. The
//!   Rust backend uses the declared name for typed-StateContext
//!   field assignment.
//! - `context_return_read_typed(lang, frame_type, system_name)` —
//!   emits the per-target downcast for `@@:return` reads. Every
//!   typed target stores the context-stack `_return` slot in
//!   an untyped slot (`Object`/`Any`/`void*`/`std::any`/…); a
//!   bare access loses the static type at the first use. This
//!   helper restores the type the user declared.
//! - `emit_handler_body_via_statements(span, source, lang, ctx)` —
//!   the AST-statement-driven splicer that scans the handler
//!   body's source bytes, classifies regions, walks the
//!   resulting Statement stream, and emits expanded native
//!   code interleaved with `generate_frame_expansion` output for
//!   every Frame segment. Handles the transition-vs-return-expr
//!   ordering, the standalone-self-call indent prefix, and the
//!   deferred self-call guard (D1 fix: fire at statement
//!   boundaries, not mid-expression).

use super::super::codegen_utils::{
    cpp_map_type, csharp_map_type, go_map_type, java_map_type, kotlin_map_type, swift_map_type,
    HandlerContext,
};
use super::utility::{normalize_indentation, split_transition_return, strip_java_unreachable};
use crate::frame_c::compiler::native_region_scanner::{FrameSegmentKind, Region};
use crate::frame_c::visitors::TargetLanguage;

/// Resolve the storage key for a positional state-arg in a transition.
/// Returns the declared param name — used by Rust backend for typed
/// StateContext struct field assignment.
pub(crate) fn resolve_state_arg_key(i: usize, target_state: &str, ctx: &HandlerContext) -> String {
    ctx.state_param_names
        .get(target_state)
        .and_then(|names| names.get(i))
        .cloned()
        .unwrap_or_else(|| i.to_string())
}

/// `@@:return` typed-read expansion across all 17 targets.
///
/// The per-call context stack's `_return` slot is untyped in every
/// typed target (`Object`/`Any`/`void*`/`std::any`/…). A bare read
/// fails the first time the result hits an arithmetic op, a typed
/// self-call arg, or a return-type-checked `return`. This helper
/// emits the target's native downcast to the handler's *declared*
/// return type, spelled the target way — type-ignorant, so it works
/// for any type the user wrote (see
/// docs/contributing/type-ignorant-codegen.md). Two targets keep a
/// per-category branch because the target language forces it: C
/// (`void*` ABI — `double` rides via the bit-pun, primitives via
/// `(intptr_t)`, pointers as-is) and Java (no `(int) Object` —
/// primitives must box-then-unbox).
///
/// Dynamic-typed targets (Python, JavaScript, Ruby, Lua, PHP, Dart,
/// GDScript) don't need a cast — they get the bare access.
pub(super) fn context_return_read_typed(
    lang: TargetLanguage,
    frame_type: &str,
    system_name: &str,
    event_name: &str,
) -> String {
    let _ = event_name; // unused on non-Rust targets
    match lang {
        TargetLanguage::Python3 | TargetLanguage::GDScript => {
            "self._context_stack[-1]._return".to_string()
        }
        TargetLanguage::TypeScript | TargetLanguage::Dart | TargetLanguage::JavaScript => {
            "this._context_stack[this._context_stack.length - 1]._return".to_string()
        }
        TargetLanguage::C => {
            // C's `_return` slot is a `void*` — the per-category unpack
            // comes from `c_marshal::c_return_read` (#72), the same
            // categorization every write site uses, so pack and unpack
            // cannot drift: `double` rides via the memcpy bit-pun, a
            // string is already a pointer, ints fit the integer width,
            // and a struct deref-copies its heap box (NOT freed here —
            // the context is still live; the interface wrapper owns the
            // single free at end-of-call).
            let raw = format!("{}_RETURN(self)", system_name);
            format!(
                "({})",
                super::super::c_marshal::c_return_read(system_name, &raw, frame_type)
            )
        }
        TargetLanguage::Rust => super::super::rust_system::rust_context_return_read_typed(
            frame_type,
            system_name,
            event_name,
        ),
        TargetLanguage::Cpp => {
            format!(
                "std::any_cast<{}>(_context_stack.back()._return)",
                cpp_map_type(frame_type)
            )
        }
        TargetLanguage::Java => {
            let raw = "_context_stack.get(_context_stack.size() - 1)._return";
            // The JVM forbids `(int) Object`; a primitive receiver has to
            // box-then-unbox. Reference types take a plain cast.
            let mapped = java_map_type(frame_type);
            let (boxed, prim): (&str, Option<&str>) = match mapped.as_str() {
                "int" => ("Integer", Some("intValue")),
                "long" => ("Long", Some("longValue")),
                "double" => ("Double", Some("doubleValue")),
                "float" => ("Float", Some("floatValue")),
                "boolean" => ("Boolean", Some("booleanValue")),
                "char" => ("Character", Some("charValue")),
                "byte" => ("Byte", Some("byteValue")),
                "short" => ("Short", Some("shortValue")),
                other => (other, None),
            };
            match prim {
                Some(m) => format!("(({}) {}).{}()", boxed, raw, m),
                None => format!("(({}) {})", boxed, raw),
            }
        }
        TargetLanguage::Kotlin => {
            let raw = "_context_stack[_context_stack.size - 1]._return";
            format!("({} as {})", raw, kotlin_map_type(frame_type))
        }
        TargetLanguage::Swift => {
            let raw = "_context_stack[_context_stack.count - 1]._return";
            format!("({} as! {})", raw, swift_map_type(frame_type))
        }
        TargetLanguage::CSharp => {
            let raw = "_context_stack[_context_stack.Count - 1]._return";
            format!("(({}) {})", csharp_map_type(frame_type), raw)
        }
        TargetLanguage::Go => {
            let raw = "s._context_stack[len(s._context_stack)-1]._return";
            format!("{}.({})", raw, go_map_type(frame_type))
        }
        TargetLanguage::Php => {
            "$this->_context_stack[count($this->_context_stack) - 1]->_return".to_string()
        }
        TargetLanguage::Ruby => "@_context_stack[@_context_stack.length - 1]._return".to_string(),
        TargetLanguage::Lua => "self._context_stack[#self._context_stack]._return".to_string(),
        TargetLanguage::Erlang => "__ReturnVal".to_string(),
        TargetLanguage::Graphviz => unreachable!(),
    }
}

/// True when the already-EMITTED output `out` ends with an unterminated
/// statement, and `lang` is a semicolon-bearing target — i.e. a `;` must be
/// spliced before the block-level Frame expansion (transition / return /
/// standalone self-call) about to be appended.
///
/// This asks a different question from [`call_segment_ends_statement`] and so
/// is correctly *backward*, over `out`, not the source. It does not guess the
/// nature of the *current* segment (that was the #116/#117 mistake — now a
/// closed forward rule); it inspects what has ALREADY been emitted to decide
/// whether the prior statement closed. `out` is the right source of truth here
/// precisely because it reflects prior *expansions*: a self-call emitted just
/// above already carries its own `;` (via `call_segment_ends_statement`), and a
/// transition above ended with `return` — the original source text would show
/// neither terminator, so reading source here would double- or mis-terminate.
/// The caller gates this on `segment_at_line_start` + `!inline_self_call`, so it
/// only fires when the expansion genuinely starts a new statement.
///
/// Python / Ruby / Lua / GDScript / Erlang use newlines or language-native
/// separators — no `;` insertion there.
///
/// The classification is exhaustive for the emitted-tail question: `;`/`{`/`}`
/// = already closed; a trailing operator / open delimiter = the emitted output
/// is mid-expression (a multi-line expression whose next line is this segment,
/// e.g. `x = a +\n@@:self.b()`), so a `;` would split it; otherwise (identifier
/// / literal / closing `)`/`]`) the statement is complete and needs `;`.
fn needs_statement_terminator(out: &str, lang: TargetLanguage) -> bool {
    let uses_semicolons = matches!(
        lang,
        TargetLanguage::Rust
            | TargetLanguage::Java
            | TargetLanguage::Kotlin
            | TargetLanguage::Swift
            | TargetLanguage::CSharp
            | TargetLanguage::C
            | TargetLanguage::Cpp
            | TargetLanguage::JavaScript
            | TargetLanguage::TypeScript
            | TargetLanguage::Php
            | TargetLanguage::Dart
            | TargetLanguage::Go
    );
    if !uses_semicolons {
        return false;
    }
    let trimmed = out.trim_end();
    if trimmed.is_empty() {
        return false;
    }
    // Pre-terminated by an explicit statement separator or a
    // block boundary — no extra `;` needed.
    if trimmed.ends_with(|c: char| matches!(c, ';' | '{' | '}')) {
        return false;
    }
    // User is mid-expression — adding `;` would split the
    // expression and break parsing.
    //
    // `)` deliberately NOT excluded: a trailing `)` from a call
    // (`foo()`) or a state-args bundle before a transition IS a
    // complete statement that needs `;`. The one case where a
    // trailing `)` is mid-expression — a C-style cast like
    // `(double) @@:self.m()` — is handled at the call site by NOT
    // injecting a terminator before an INLINE self-call (see the
    // `inline_self_call` gate in `emit_handler_body_via_statements`);
    // that path never reaches this heuristic. So treating `)` as
    // statement-complete here is correct for every case that does.
    if trimmed.ends_with(|c: char| {
        matches!(
            c,
            '=' | '+'
                | '-'
                | '*'
                | '/'
                | '%'
                | '<'
                | '>'
                | '&'
                | '|'
                | '^'
                | '!'
                | '?'
                | ','
                | '('
                | '['
                | ':'
                | '.'
        )
    }) {
        return false;
    }
    // Closing parens / brackets / identifiers / literals: the
    // expression is complete; statement needs a `;` to terminate.
    true
}

/// Forward-looking statement-terminator test for a Frame **call** segment
/// (`@@:self.method()` or `@@:self.field.method()`): does this call end a
/// statement — so it needs a trailing `;` on semicolon targets — or is it
/// embedded in a larger construct (a condition, an argument, an operand, an
/// assignment that continues) where a synthesized `;` would break parsing?
///
/// The decisive choice is *direction*. The test looks at what FOLLOWS the call
/// in the source (`body_bytes` from `seg_end`), not what precedes it. "Is this
/// a statement?" has a **closed** forward characterization — a call ends a
/// statement iff nothing continues the expression after it on its source line —
/// whereas the backward view (what precedes the call) is open-ended: it has to
/// enumerate every expression context, parens then `if`/`while`/`for`/`switch`,
/// then Swift `guard`, Rust `while let`, … target by target, and so can never
/// be proven complete. The earlier backward scanner shipped exactly that
/// incompleteness, as #116 then #117.
///
/// A call is the line's last token — a statement terminator — iff the next
/// source content past horizontal whitespace (and an optional trailing line
/// comment) is a line break, a block close `}`, or end-of-body. Anything else —
/// `)`, `{`, `.`, `,`, an operator, `==`, or a user-written `;` — means the
/// expression continues, or is already terminated, so no `;` is synthesized.
/// (A user-written `;` directly after the call is thus left untouched: no
/// doubling.) This single rule covers a bare call statement (`@@:self.tick()`),
/// an assignment that ends in a call (`x = @@:self.reading()`), and correctly
/// declines a call in a condition (`if (@@:self.alive())`,
/// `if size >= @@:self.len() {`) or argument position (`f(@@:self.g())`).
///
/// Boundary contract (Oceans model): framec terminates a line only when its
/// own emitted call is that line's last token. A call written *mid*-expression
/// with a native tail (`int y = @@:self.f() + 1`) is not terminated here — the
/// `;` after the native tail is the author's to write, since native is
/// passthrough. One Frame statement-call per source line.
fn call_segment_ends_statement(body_bytes: &[u8], seg_end: usize) -> bool {
    let n = body_bytes.len();
    let mut i = seg_end;
    // Skip horizontal whitespace — but NOT a newline, which ends the line.
    while i < n && matches!(body_bytes[i], b' ' | b'\t') {
        i += 1;
    }
    if i >= n {
        return true; // end of handler body
    }
    match body_bytes[i] {
        // Line break or block close: the call is the line's last token.
        b'\n' | b'\r' | b'}' => true,
        // A trailing line comment ends the line too (`call() // note`).
        b'#' => true,
        b'/' if i + 1 < n && body_bytes[i + 1] == b'/' => true,
        // `)`, `{`, `.`, `,`, operators, `==`, a user `;`, any other token:
        // the expression continues (or is already terminated) — no `;`.
        _ => false,
    }
}

/// Emit handler body by scanning for Frame segments and walking them as AST statements.
///
/// Pipeline: source bytes → scanner → regions → statements → expansion walk → output string.
/// NativeCode passes through verbatim; Frame constructs are expanded per-language.
pub(crate) fn emit_handler_body_via_statements(
    span: &crate::frame_c::compiler::ast::Span,
    source: &[u8],
    lang: TargetLanguage,
    ctx: &HandlerContext,
) -> String {
    use crate::frame_c::compiler::frame_ast::Statement;
    use crate::frame_c::compiler::native_region_scanner::regions_to_statements;

    if span.start >= source.len() || span.end > source.len() || span.start >= span.end {
        return String::new();
    }

    let body_bytes = &source[span.start..span.end];
    let open_brace = match body_bytes.iter().position(|&b| b == b'{') {
        Some(pos) => pos,
        None => return String::from_utf8_lossy(body_bytes).trim().to_string(),
    };

    // Scanner does the hard work
    let mut scanner = super::scanner_dispatch::get_native_scanner(lang);
    let scan_result = match scanner.scan(body_bytes, open_brace) {
        Ok(r) => r,
        Err(_) => return String::new(),
    };

    // Convert regions to typed AST statements
    let statements = regions_to_statements(body_bytes, &scan_result.regions);

    // Walk statements — NativeCode passes through, Frame constructs get expanded.
    // We still call generate_frame_expansion() for Frame constructs by looking up
    // the original Region to get the span/kind/metadata/indent it needs.
    let mut out = String::new();
    let mut frame_idx = 0usize; // Index into FrameSegment regions
    let frame_regions: Vec<_> = scan_result
        .regions
        .iter()
        .filter(|r| matches!(r, Region::FrameSegment { .. }))
        .collect();

    // Track which statement indices to skip (consumed by lookahead)
    let mut skip_set = std::collections::HashSet::new();
    // Deferred self-call transition guard — emitted after the native
    // line containing the self-call completes (so `;` lands before guard).
    let mut pending_guard: Option<String> = None;

    for (stmt_idx, stmt) in statements.iter().enumerate() {
        if skip_set.contains(&stmt_idx) {
            // This statement was consumed by a prior lookahead — skip it
            // but still advance frame_idx if it's a Frame statement
            if !matches!(stmt, Statement::NativeCode(_)) {
                frame_idx += 1;
            }
            continue;
        }
        match stmt {
            Statement::NativeCode(text) => {
                if let Some(guard) = pending_guard.take() {
                    // Tight option 2 (D1 fix): the transition guard must
                    // fire AT A STATEMENT BOUNDARY, not mid-expression.
                    // The boundary is signaled by a newline in the next
                    // NativeCode segment — when present, we've reached
                    // end-of-line and the assignment is complete.
                    //
                    // If the next NativeCode lacks a newline, it's a
                    // continuation of the same statement (e.g. ` + 5`,
                    // `, lit`, ` && other`). Emitting the guard here
                    // would split the expression. Re-stash the guard
                    // and let it propagate to the NEXT NativeCode that
                    // DOES end the line. A subsequent self-call segment
                    // may overwrite the guard with its own — that's
                    // correct: `_transitioned` is monotonic, so a single
                    // statement-end check catches "any embedded call
                    // transitioned the system".
                    if let Some(nl_pos) = text.find('\n') {
                        out.push_str(&text[..=nl_pos]);
                        out.push_str(&guard);
                        out.push('\n');
                        out.push_str(&text[nl_pos + 1..]);
                    } else {
                        // No newline — keep guard pending; emit text only.
                        out.push_str(text);
                        pending_guard = Some(guard);
                    }
                } else {
                    out.push_str(text);
                }
            }
            _ => {
                // Look up the corresponding original Region for expansion parameters
                if frame_idx < frame_regions.len() {
                    if let Region::FrameSegment {
                        span: seg_span,
                        kind,
                        indent,
                        metadata,
                    } = frame_regions[frame_idx]
                    {
                        let expansion = super::generate_frame_expansion(
                            body_bytes, seg_span, *kind, *indent, lang, ctx, metadata,
                        );

                        // ── Transition control flow ──────────────────────
                        // Transition expansions end with `return` to exit the
                        // handler after the state change. But if a return-expr
                        // (`@@:(expr)`) follows the transition in the same
                        // scope, the return makes it unreachable.
                        //
                        // Fix: separate the expansion body from the trailing
                        // `return`. Emit body, consume any same-scope
                        // return-expr, then emit `return`. This is a clean
                        // separation of transition semantics (the expansion)
                        // from handler control flow (the orchestrator).
                        let is_transition = matches!(
                            kind,
                            FrameSegmentKind::Transition
                                | FrameSegmentKind::Forward
                                | FrameSegmentKind::StackPush
                                | FrameSegmentKind::StackPop
                        );
                        // RFC-0033 #21: Frame-segment kinds that
                        // expand to block-level statements. When one
                        // of these follows user NativeCode that lacks
                        // a trailing terminator, the prior statement
                        // needs `;` inserted (in semicolon-bearing
                        // targets). The `needs_statement_terminator`
                        // helper excludes mid-expression positions
                        // (after `=`, binary operators, open
                        // delimiters) where a `;` would break parsing.
                        // An INLINE self-call (e.g. `x = (double) @@:self.m()`)
                        // continues the current expression — it is not a new
                        // statement, so the prior-statement terminator must not
                        // be injected. Without this gate, `needs_statement_terminator`
                        // sees the trailing `)` of a `(double)` cast, mistakes it
                        // for a complete statement, and splices `;` mid-expression
                        // (`x = (double); this.m()`), which fails to compile on
                        // C#/Java/etc. A self-call is standalone (a real new
                        // statement) only when it starts its own line.
                        // `out.ends_with('\n')` is intentional and correct here:
                        // this asks whether the EMITTED cursor sits at a line
                        // start (so the self-call begins its own output line),
                        // which is an output-position question, not the
                        // segment-nature guess that #116/#117 was. A self-call
                        // that begins its own line is a standalone statement; one
                        // with emitted content before it on the line (e.g. the
                        // cast operand `x = (double) @@:self.m()`) is inline.
                        let inline_self_call = matches!(kind, FrameSegmentKind::ContextSelfCall)
                            && !(out.is_empty() || out.ends_with('\n'));
                        if (is_transition
                            || matches!(
                                kind,
                                FrameSegmentKind::ContextSelfCall
                                    | FrameSegmentKind::ReturnStatement
                                    | FrameSegmentKind::ReturnCall
                            ))
                            && !inline_self_call
                            && needs_statement_terminator(&out, lang)
                        {
                            let last_non_ws = out.rfind(|c: char| !c.is_whitespace());
                            if let Some(pos) = last_non_ws {
                                let tail: String = out[pos + 1..].to_string();
                                out.truncate(pos + 1);
                                out.push(';');
                                out.push_str(&tail);
                            }
                        }
                        if is_transition {
                            let (body, return_kw) = split_transition_return(&expansion);
                            // Multi-line expansion on same line as native code
                            // needs a line break first
                            if !out.is_empty() && !out.ends_with('\n') && body.contains('\n') {
                                out.push('\n');
                            }
                            out.push_str(body);

                            if !return_kw.is_empty() {
                                // Scan forward for a return-expr that directly
                                // follows this transition in the same block.
                                //
                                // Two conditions must BOTH hold:
                                // 1. Only whitespace NativeCode between them
                                //    (content like `else:` or `}` = different block)
                                // 2. The return-expr has the same scanner-computed
                                //    indent as the transition (catches Python's
                                //    indent-based scoping where dedent is whitespace)
                                //
                                // Together these handle both brace languages
                                // (content stops the scan) and indent languages
                                // (indent mismatch stops the consume).
                                //
                                // For nested-control-flow transitions where
                                // the user expected an outer-scope
                                // `@@:(value)` to apply on the transition path,
                                // see W705 in `frame_validator.rs` — the spec
                                // is "code after a transition is unreachable",
                                // and the validator warns when the user's
                                // pattern would silently leak a default value
                                // (Issue #4 in FRAMEC_BUGS.md).
                                let next_frame_idx = frame_idx + 1;
                                for j in (stmt_idx + 1)..statements.len() {
                                    match &statements[j] {
                                        Statement::NativeCode(text) if text.trim().is_empty() => {
                                            continue
                                        }
                                        Statement::ContextReturnExpr { .. }
                                        | Statement::ReturnCall { .. } => {
                                            if next_frame_idx < frame_regions.len() {
                                                if let Region::FrameSegment {
                                                    span: ret_span,
                                                    kind: ret_kind,
                                                    indent: ret_indent,
                                                    metadata: ret_meta,
                                                } = frame_regions[next_frame_idx]
                                                {
                                                    if *ret_indent == *indent {
                                                        let ret_exp =
                                                            super::generate_frame_expansion(
                                                                body_bytes,
                                                                ret_span,
                                                                *ret_kind,
                                                                *ret_indent,
                                                                lang,
                                                                ctx,
                                                                ret_meta,
                                                            );
                                                        out.push('\n');
                                                        out.push_str(&ret_exp);
                                                        skip_set.insert(j);
                                                    }
                                                }
                                            }
                                            break;
                                        }
                                        _ => break,
                                    }
                                }
                                // Emit return on its own line at the transition's indent
                                out.push('\n');
                                out.push_str(&" ".repeat(*indent));
                                out.push_str(return_kw);
                            }
                        } else {
                            // ── Non-transition expansion ─────────────────
                            if !out.is_empty() && !out.ends_with('\n') && expansion.contains('\n') {
                                out.push('\n');
                            }
                            // Self-call: a bare call statement that starts its
                            // own line needs an indent prefix (the preceding
                            // native newline carries no indentation for it). A
                            // field call takes its indent from the preserved
                            // native whitespace, so it needs no prefix.
                            // Output-position check (see `inline_self_call`): the
                            // indent prefix depends on whether the EMITTED cursor
                            // is at a line start — correct here especially for
                            // indentation-sensitive targets (GDScript/Python),
                            // where it must follow prior multi-line expansions'
                            // output, not the segment's source column.
                            let is_standalone_self_call = *kind
                                == FrameSegmentKind::ContextSelfCall
                                && (out.is_empty() || out.ends_with('\n'));
                            // Statement termination (#116/#117 + the latent
                            // assignment-ending-in-call cases) is decided FORWARD
                            // — on what follows the call in the source, not what
                            // precedes it. A same-system self call or a
                            // cross-system field call ends a statement iff nothing
                            // continues the expression after it on its line. The
                            // per-language `;` (or no-op for Python/Ruby/Lua/
                            // GDScript) is applied by the match below.
                            let terminate_call =
                                matches!(
                                    kind,
                                    FrameSegmentKind::ContextSelfCall
                                        | FrameSegmentKind::ContextSelfFieldCall
                                ) && call_segment_ends_statement(body_bytes, seg_span.end);
                            if is_standalone_self_call {
                                out.push_str(&" ".repeat(*indent));
                            }
                            out.push_str(&expansion);
                            if terminate_call {
                                match lang {
                                    // Statement-terminator-free targets: newlines
                                    // (Python/Ruby/Lua/GDScript) or `,`/`.` clause
                                    // separators (Erlang). A `;` here is a syntax
                                    // error on these — Erlang's `;` separates
                                    // clauses, not statements.
                                    TargetLanguage::Python3
                                    | TargetLanguage::GDScript
                                    | TargetLanguage::Ruby
                                    | TargetLanguage::Lua
                                    | TargetLanguage::Erlang
                                    | TargetLanguage::Graphviz => {}
                                    _ => out.push(';'),
                                }
                            }

                            // Self-call guard — deferred until native line ends.
                            // RFC-0046: a `@@:self.<action>(...)` call is a direct
                            // action call (not a kernel-dispatched interface call),
                            // so it cannot transition and must NOT get the guard.
                            let is_action_call = matches!(
                                metadata,
                                crate::frame_c::compiler::native_region_scanner::SegmentMetadata::SelfCall { method, .. }
                                    if ctx.actions.contains(method)
                            );
                            if *kind == FrameSegmentKind::ContextSelfCall && !is_action_call {
                                let guard = super::generate_self_call_guard(
                                    *indent,
                                    lang,
                                    &ctx.system_name,
                                );
                                if !guard.is_empty() {
                                    pending_guard = Some(guard);
                                }
                            }
                        }
                    }
                    frame_idx += 1;
                }
            }
        }
    }

    // Flush any remaining deferred self-call guard
    if let Some(guard) = pending_guard.take() {
        out.push('\n');
        out.push_str(&guard);
    }

    // Same post-processing as splice path
    let text = out.trim_start_matches('\n').trim_end();
    let text = normalize_indentation(text);
    if matches!(
        lang,
        TargetLanguage::Java
            | TargetLanguage::Kotlin
            | TargetLanguage::Swift
            | TargetLanguage::CSharp
            | TargetLanguage::Go
    ) {
        let text = if matches!(
            lang,
            TargetLanguage::Swift | TargetLanguage::Kotlin | TargetLanguage::Go
        ) {
            text.lines()
                .map(|line| {
                    let trimmed = line.trim_end();
                    if trimmed.ends_with(';') {
                        let stripped = trimmed.trim_end_matches(';');
                        if stripped.is_empty() && line.trim() == ";" {
                            String::new()
                        } else {
                            stripped.to_string()
                        }
                    } else {
                        line.to_string()
                    }
                })
                .collect::<Vec<_>>()
                .join("\n")
        } else {
            // String/comment-safe: a literal `";;"` in user code must keep both
            // semicolons; only collapse a real double-terminator.
            super::super::codegen_utils::replace_outside_strings_and_comments(
                &text,
                lang,
                &[(";;", ";")],
            )
        };
        strip_java_unreachable(&text)
    } else {
        text
    }
}

/// Expand only the `@@:self.*` constructs in an action or operation body
/// (RFC-0046), returning the brace-stripped inner code.
///
/// Action and operation bodies are native passthrough — they are NOT routed
/// through `emit_handler_body_via_statements`, so without this pass
/// `@@:self.field`, `@@:self.field.method()`, and `@@:self.method()` would leak
/// verbatim into the target. This scans the body span (boundary-safe: the
/// scanner skips strings/comments), replaces each self segment with its
/// per-target expansion, and leaves every other construct — `@@:(expr)`,
/// `@@:system.state`, native code — untouched for the textual
/// `expand_system_state_in_code` pass that runs next. No transition guard is
/// emitted (action/operation bodies don't transition).
pub(crate) fn expand_self_in_body(
    span: &crate::frame_c::compiler::frame_ast::Span,
    source: &[u8],
    lang: TargetLanguage,
    ctx: &HandlerContext,
) -> String {
    if span.start >= source.len() || span.end > source.len() || span.start >= span.end {
        return String::new();
    }
    let body_bytes = &source[span.start..span.end];
    super::super::interface_gen::strip_body_braces(&lower_self_in_code(body_bytes, lang, ctx))
}

/// Scan a `{ … }` body and lower every `@@:self.*` to its per-target form,
/// returning the code with its braces still present (the caller strips them).
/// Self constructs nested inside a native `return <expr>` (action/operation
/// bodies use native `return`) are lowered too — the scanner groups
/// `return @@:self.x` into one `ReturnStatement` segment, so the `@@:self`
/// inside it is reached by recursively lowering the return expression.
fn lower_self_in_code(body_bytes: &[u8], lang: TargetLanguage, ctx: &HandlerContext) -> String {
    use crate::frame_c::compiler::native_region_scanner::RegionSpan;

    let open_brace = match body_bytes.iter().position(|&b| b == b'{') {
        Some(p) => p,
        None => return String::from_utf8_lossy(body_bytes).to_string(),
    };
    let mut scanner = super::scanner_dispatch::get_native_scanner(lang);
    let scan_result = match scanner.scan(body_bytes, open_brace) {
        Ok(r) => r,
        Err(_) => return String::from_utf8_lossy(body_bytes).to_string(),
    };

    let mut edits: Vec<(usize, usize, String)> = Vec::new();
    for region in &scan_result.regions {
        if let Region::FrameSegment {
            span: rspan,
            kind,
            indent,
            metadata,
        } = region
        {
            let RegionSpan { start, end } = *rspan;
            match kind {
                FrameSegmentKind::ContextSelf
                | FrameSegmentKind::ContextSelfFieldCall
                | FrameSegmentKind::ContextSelfCall => {
                    let expansion = super::generate_frame_expansion(
                        body_bytes, rspan, *kind, *indent, lang, ctx, metadata,
                    );
                    edits.push((start, end, expansion));
                }
                FrameSegmentKind::ReturnStatement => {
                    // Native `return <expr>` in an action/operation body: keep
                    // `return` native, lower any `@@:self.*` in `<expr>`.
                    let seg = String::from_utf8_lossy(&body_bytes[start..end]);
                    if let Some(rest) = seg.trim_start().strip_prefix("return") {
                        if rest.contains("@@:self") {
                            let exp = format!("return{}", lower_self_in_fragment(rest, lang, ctx));
                            edits.push((start, end, exp));
                        }
                    }
                }
                FrameSegmentKind::ContextReturnExpr => {
                    // `@@:(<expr>)`: the textual `expand_system_state_in_code`
                    // pass lowers the `@@:(…)` wrapper to `return …`, but it does
                    // not descend into `<expr>`. Lower any `@@:self.*` in `<expr>`
                    // here, preserving the `@@:(…)` wrapper for that pass.
                    let seg = String::from_utf8_lossy(&body_bytes[start..end]).to_string();
                    if let Some(open) = seg.find("@@:(") {
                        let inner_start = open + "@@:(".len();
                        let b = seg.as_bytes();
                        let mut depth = 1i32;
                        let mut j = inner_start;
                        while j < b.len() && depth > 0 {
                            match b[j] {
                                b'(' => depth += 1,
                                b')' => depth -= 1,
                                _ => {}
                            }
                            if depth > 0 {
                                j += 1;
                            }
                        }
                        if j <= b.len() && seg[inner_start..j].contains("@@:self") {
                            let lowered = lower_self_in_fragment(&seg[inner_start..j], lang, ctx);
                            let rebuilt =
                                format!("{}@@:({}){}", &seg[..open], lowered, &seg[j + 1..]);
                            edits.push((start, end, rebuilt));
                        }
                    }
                }
                _ => {}
            }
        }
    }

    // Apply replacements right-to-left so earlier byte offsets stay valid.
    let mut code = String::from_utf8_lossy(body_bytes).to_string();
    edits.sort_by_key(|e| std::cmp::Reverse(e.0));
    for (start, end, exp) in edits {
        if start <= end
            && end <= code.len()
            && code.is_char_boundary(start)
            && code.is_char_boundary(end)
        {
            code.replace_range(start..end, &exp);
        }
    }
    code
}

/// Lower `@@:self.*` inside a code fragment that has no surrounding braces
/// (e.g. a return expression). Wraps it in `{ … }` so the scanner has a body,
/// reuses `lower_self_in_code`, then unwraps. The fragment carries no native
/// `return`, so the recursion terminates after one level.
fn lower_self_in_fragment(fragment: &str, lang: TargetLanguage, ctx: &HandlerContext) -> String {
    if !fragment.contains("@@:self") {
        return fragment.to_string();
    }
    let wrapped = format!("{{{}}}", fragment);
    let lowered = lower_self_in_code(wrapped.as_bytes(), lang, ctx);
    let t = lowered.trim_start();
    let inner = t
        .strip_prefix('{')
        .map(|s| s.strip_suffix('}').unwrap_or(s))
        .unwrap_or(&lowered);
    inner.to_string()
}

#[cfg(test)]
mod statement_terminator_tests {
    // Statement-terminator coverage for Frame call segments. Termination is
    // decided FORWARD (`call_segment_ends_statement`): a self/field call ends a
    // statement — and gets a `;` on semicolon targets — iff nothing continues
    // the expression after it on its source line. The cases below pin every
    // position that occurs in real Frame source (verified against the matrix
    // fixtures + frame-games): bare statement (#116), assignment-ending-in-call,
    // call-as-argument, condition (#117 incl. paren-less), and a user-written
    // `;` (must not double).
    use crate::run;

    fn cs(body: &str) -> String {
        let src = format!(
            "@@[target(\"csharp\")]\n\
             @@system Ship {{\n\
             \x20   interface:\n\
             \x20       a()\n\
             \x20       b(n: int)\n\
             \x20       reading(): int\n\
             \x20       alive(): bool\n\
             }}\n\
             @@[main]\n\
             @@system Game {{\n\
             \x20   interface:\n\
             \x20       run()\n\
             \x20   machine:\n\
             \x20       $S {{ run() {{ {body} }} }}\n\
             \x20   domain:\n\
             \x20       ship: Ship = @@Ship()\n\
             }}\n"
        );
        run(&src, "csharp")
    }

    // #117: paren-less-`if` targets (Go/Rust/Swift/Kotlin) — a value-returning
    // call in a condition must NOT be terminated (`if s.ship.alive() {`, not
    // `if s.ship.alive(); {`).
    fn go(body: &str) -> String {
        let src = format!(
            "@@[target(\"go\")]\n\
             package main\n\
             @@system Ship {{\n\
             \x20   interface:\n\
             \x20       alive(): bool\n\
             \x20       tick()\n\
             }}\n\
             @@[main]\n\
             @@system Game {{\n\
             \x20   interface:\n\
             \x20       run()\n\
             \x20   machine:\n\
             \x20       $S {{ run() {{ {body} }} }}\n\
             \x20   domain:\n\
             \x20       ship: Ship = @@Ship()\n\
             }}\n"
        );
        run(&src, "go")
    }

    #[test]
    fn go_call_in_paren_less_if_condition_not_terminated() {
        let out = go("if @@:self.ship.alive() { return }");
        assert!(
            out.contains("if s.ship.Alive() {"),
            "stray `;` in a paren-less if-condition:\n{out}"
        );
        assert!(
            !out.contains("Alive();"),
            "spurious `;` spliced into an if-condition call:\n{out}"
        );
    }

    #[test]
    fn void_field_call_statement_is_terminated() {
        // (A) bare statement, last token on its line.
        let out = cs("@@:self.ship.a()");
        assert!(
            out.contains("this.ship.a();"),
            "void call statement not terminated:\n{out}"
        );
    }

    #[test]
    fn assignment_ending_in_field_call_is_terminated() {
        // (B) the call is the last token of an assignment → `;` after it.
        let out = cs("int x = @@:self.ship.reading()");
        assert!(
            out.contains("int x = this.ship.reading();"),
            "assignment ending in a call not terminated:\n{out}"
        );
    }

    #[test]
    fn field_call_as_argument_not_terminated() {
        // (D) outer call ends the line (terminated); the inner call is an
        // argument (followed by `)`), so it must NOT be terminated.
        let out = cs("@@:self.ship.b(@@:self.ship.reading())");
        assert!(
            out.contains("this.ship.b(this.ship.reading());"),
            "outer call not terminated / inner arg mis-terminated:\n{out}"
        );
        assert!(
            !out.contains("reading();)"),
            "spurious `;` spliced into an argument-position call:\n{out}"
        );
    }

    #[test]
    fn value_field_call_in_condition_not_terminated() {
        // (C) a value call in a condition (followed by `)`) keeps no `;`.
        let out = cs("if (@@:self.ship.alive()) { }");
        assert!(
            out.contains("if (this.ship.alive())"),
            "expected un-terminated call inside if():\n{out}"
        );
        assert!(
            !out.contains("this.ship.alive();"),
            "spurious `;` spliced into an expression-position call:\n{out}"
        );
    }

    #[test]
    fn user_written_semicolon_is_not_doubled() {
        // A `;` the author already wrote (next token after the call) is left
        // alone — the forward rule sees a non-line-end and declines.
        let out = cs("@@:self.ship.a();");
        assert!(
            out.contains("this.ship.a();") && !out.contains("this.ship.a();;"),
            "user-written `;` was doubled:\n{out}"
        );
    }

    #[test]
    fn double_semicolon_inside_string_literal_preserved() {
        // The `;;`→`;` post-processing collapse must not touch a `;;` that is
        // part of a string literal.
        let out = cs("string sep = \"a;;b\";");
        assert!(
            out.contains("\"a;;b\""),
            "`;;` collapsed inside a string literal:\n{out}"
        );
    }
}
