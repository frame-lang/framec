//! Async dispatch-chain transformer.
//!
//! When any interface method on a Frame system is declared `async`,
//! the entire generated dispatch chain (interface methods, kernel,
//! router, state dispatch, handlers) has to be async too. This
//! module post-processes the `CodegenNode::Class` tree to enforce
//! that contract — flipping `is_async: true` on the relevant
//! methods, injecting `await` / `co_await` / `.await` into internal
//! dispatch call sites, and emitting an `init()` method that fires
//! the start-state's `$>` event.
//!
//! ## Per-language quirks worth flagging
//!
//! - **Java** is special-cased: it has no native async/await. The
//!   `make_java_interface_async` path wraps async-declared interface
//!   methods in `CompletableFuture<T>` and keeps the internal
//!   dispatch chain synchronous. An `init()` method is still emitted
//!   for cross-language API parity, but its body is a no-op.
//! - **Swift** reserves `init` for constructors — `func init() async`
//!   is a parse error. We rename to `initAsync` on Swift so tests can
//!   write `await w.initAsync()`.
//! - **C++** uses `co_await` (not `await`). Per-handler methods that
//!   don't actually `co_await` anything need a trailing `co_return;`
//!   so `FrameTask<T>` is constructed as a real coroutine — otherwise
//!   the caller's `co_await` crashes on the empty handle.
//! - **Rust** uses postfix `.await` rather than the `await <expr>`
//!   prefix used everywhere else. The `insert_rust_await` helper
//!   finds the closing paren of the dispatch call and splices
//!   `.await` after it.
//! - **Persist machinery stays sync.** `save_state` / `restore_state`
//!   and their recursive helpers (`__serComp`, `__deserComp`,
//!   `__convertJsonValue` and case variants) are pure data
//!   operations — they don't dispatch events. Marking them async
//!   triggered defects D16 (Swift's `await`-pattern check missed the
//!   serializer-internal calls) and D17 (C++ rewrote `return` to
//!   `co_return` inside the embedded `__ser` lambda whose return
//!   type isn't a coroutine type).
//!
//! Only `make_system_async` is part of this module's pub API; the
//! rest are private helpers.

use super::{CodegenNode, SystemAst, TargetLanguage, Visibility};

/// Java-specific async handling. Java has no native async/await; the
/// approximation is:
///
/// - Public interface methods declared `async` get `is_async = true`,
///   which the Java backend honors by wrapping the return type in
///   `CompletableFuture<T>` and the body in
///   `CompletableFuture.completedFuture(...)`.
/// - The internal dispatch chain (`__kernel`, `__router`, state
///   functions, transitions, cascades) stays synchronous. Users pay
///   `.get()` only at the interface boundary; deep
///   `.get()`-everywhere chains would be noisy and would not buy
///   concurrency since `CompletableFuture.completedFuture` is
///   already-resolved by construction.
/// - The constructor fires the start-state's `$>` synchronously (the
///   default Java emission). Other async backends defer this to a
///   separate `init()` so the caller can `await` it; Java's sync
///   internals make that two-phase split unnecessary.
/// - An `init()` method is still emitted for API parity with the
///   other async backends — so a user can write
///   `system.init().get()` portably across languages — but its body
///   is a no-op (returns an already-completed future). The
///   constructor has already done the work.
pub(super) fn make_java_interface_async(class_node: &mut CodegenNode, system: &SystemAst) {
    let async_names: std::collections::HashSet<String> = system
        .interface
        .iter()
        .filter(|m| m.is_async)
        .map(|m| m.name.clone())
        .collect();
    if let CodegenNode::Class {
        ref mut methods, ..
    } = class_node
    {
        for method in methods.iter_mut() {
            if let CodegenNode::Method { is_async, name, .. } = method {
                if async_names.contains(name) {
                    *is_async = true;
                }
            }
        }

        // Emit `init()` as an API-parity no-op. The constructor
        // already drove the start-state cascade.
        let _ = system;
        methods.push(CodegenNode::Method {
            name: "init".to_string(),
            params: vec![],
            return_type: None,
            body: vec![CodegenNode::NativeBlock {
                code: "return java.util.concurrent.CompletableFuture.completedFuture(null);"
                    .to_string(),
                span: None,
            }],
            is_async: true,
            is_static: false,
            visibility: Visibility::Public,
            decorators: vec![],
        });
    }
}

/// Transform a generated system class to use async dispatch.
///
/// When any interface method is declared `async`, the entire dispatch chain
/// (interface methods, kernel, router, state dispatch, handlers) must be async.
/// This post-processes the CodegenNode tree to:
/// 1. Set `is_async: true` on all non-static, non-constructor methods
/// 2. The backends handle `is_async` to emit `async def` / `async` / etc.
///
/// Note: `await` on internal dispatch calls is handled by the backends
/// recognizing Await nodes in the method bodies, or via NativeBlock code
/// that already contains the dispatch calls.
pub(crate) fn make_system_async(
    class_node: &mut CodegenNode,
    _system_name: &str,
    lang: TargetLanguage,
    system: &crate::frame_c::compiler::frame_ast::SystemAst,
) {
    // Operations the user did NOT mark `async` MUST stay synchronous —
    // they're explicitly non-dispatching, so coroutinizing them yields a
    // method that returns a coroutine the caller has to await for no
    // real reason. The pre-RFC-0043 single-class architecture coroutinized
    // them anyway (because every external call was awaited), but RFC-0043's
    // casing exposes the asymmetry: a sync casing delegate calling an
    // async machine op returns an unawaited coroutine. Honor the user's
    // declaration instead.
    let sync_operation_names: std::collections::HashSet<&str> = system
        .operations
        .iter()
        .filter(|o| !o.is_async)
        .map(|o| o.name.as_str())
        .collect();

    // Java path is structurally different — interface methods only, sync
    // internals, no-op init().
    if let CodegenNode::Class {
        ref mut methods,
        ref name,
        ..
    } = class_node
    {
        let system_name = name.clone();
        for method in methods.iter_mut() {
            if let CodegenNode::Method {
                is_async,
                is_static,
                name,
                body,
                ..
            } = method
            {
                // Skip static methods and constructors
                if *is_static || name == "__init__" || name == "new" {
                    continue;
                }
                // Skip __transition (synchronous compartment swap, no dispatch)
                if name == "__transition" {
                    continue;
                }
                // Skip __prepareEnter / __prepareExit — pure data
                // operations on Compartment objects, no event dispatch.
                // Keeping them sync lets the constructor (which can't be
                // async in JS/TS) call __prepareEnter directly.
                if name == "__prepareEnter" || name == "__prepareExit" || name == "__hsm_chain" {
                    continue;
                }
                // Skip persist machinery — save_state / restore_state and
                // the recursive helpers (__serComp / __deserComp /
                // __convertJsonValue and case-variant aliases) are pure
                // data operations on the compartment chain. They don't
                // dispatch events. Defects D16 (Swift) and D17 (cpp_23)
                // surfaced here: marking save_state async on Swift caused
                // its calls to __serComp/__deserComp to lack the matching
                // `await` (those calls don't match the dispatch-call
                // pattern in `add_await_to_string`); on cpp_23, marking
                // save_state as a coroutine triggered the
                // `rewrite_return_to_co_return` pass to also rewrite
                // `return` statements inside the embedded `__ser` lambda,
                // breaking compile because the lambda's nlohmann::json
                // return type isn't a coroutine type. Keeping persist
                // sync resolves both.
                if name == "save_state"
                    || name == "saveState"
                    || name == "SaveState"
                    || name == "restore_state"
                    || name == "restoreState"
                    || name == "RestoreState"
                    || name == "__serComp"
                    || name == "__SerComp"
                    || name == "__deserComp"
                    || name == "__DeserComp"
                    || name == "__convertJsonValue"
                    || name == "__convertJsonArray"
                    || name == "__convertJsonObject"
                    || name == "_restore"
                {
                    continue;
                }
                // Skip user-sync operations — they're non-dispatching by
                // declaration; coroutinizing them would force callers to
                // await for no real reason and break the RFC-0043 casing's
                // sync op delegate.
                if sync_operation_names.contains(name.as_str()) {
                    continue;
                }
                *is_async = true;
                // #158: `await`/`co_await`/`.await` on internal dispatch
                // calls is emitted AT GENERATION (machinery/*, interface_gen,
                // state_dispatch, rust_system consult `is_async_layered()`),
                // so no post-pass rescans the emitted text. Verified by the
                // 17 async snapshots staying byte-identical when the old
                // rewriter was deleted.
                // C++ coroutines: per-handler methods (`_s_<…>_hdl_<…>`) may
                // lack a terminating co_await / co_return / co_yield (e.g.
                // a lifecycle enter that just runs native code). For those,
                // append `co_return;` so the function is actually a
                // coroutine — otherwise `FrameTask<void>` is returned by
                // value without a backing promise, and the caller's
                // `co_await` crashes.
                if matches!(lang, TargetLanguage::Cpp)
                    && (name.starts_with("_s_") || name.starts_with("_state_"))
                {
                    ensure_cpp_coroutine_terminator(body);
                }
            }
            if let CodegenNode::Constructor { body, .. } = method {
                // Constructor stays sync — remove kernel call for async systems
                // (user calls `await system.init()` instead)
                remove_kernel_call_from_body(body);
            }
        }

        // Add async init() method — fires $> enter event
        let init_code = match lang {
            TargetLanguage::Python3 => format!(
                r#"__e = {s}FrameEvent("$>", None)
__ctx = {s}FrameContext(__e, None)
self._context_stack.append(__ctx)
await self.__kernel(__e)
self._context_stack.pop()"#,
                s = system_name
            ),
            TargetLanguage::TypeScript | TargetLanguage::JavaScript => format!(
                // FrameEvent's second param is the parameters list (any[]
                // in TS). Pass an empty array, not null — strict-mode TS
                // (D-TS-1) rejects null where any[] is expected.
                r#"const __e = new {s}FrameEvent("$>", []);
const __ctx = new {s}FrameContext(__e, null);
this._context_stack.push(__ctx);
await this.__kernel(__e);
this._context_stack.pop();"#,
                s = system_name
            ),
            TargetLanguage::Rust => format!(
                // RFC-0020: __kernel takes &Rc<FrameEvent>; FrameContext
                // holds an Rc-wrapped event. Wrap the synthesized $>
                // before pushing the context and dispatching.
                // RFC-0025 Track B.1: $> is the FrameEnter variant
                // (lifecycle args are empty for the no-args async case).
                r#"let __e = alloc::rc::Rc::new({s}FrameEvent::FrameEnter {{}});
let __ctx = {s}FrameContext::new(alloc::rc::Rc::clone(&__e), None);
self._context_stack.push(__ctx);
self.__kernel(&__e).await;
self._context_stack.pop();"#,
                s = system_name
            ),
            // Async not supported for these targets — emit a comment placeholder
            TargetLanguage::C => format!("// async not supported for C"),
            TargetLanguage::Cpp => format!(
                r#"{s}FrameEvent __e("$>");
{s}FrameContext __ctx(std::move(__e));
_context_stack.push_back(std::move(__ctx));
co_await __kernel(_context_stack.back()._event);
_context_stack.pop_back();
co_return;"#,
                s = system_name
            ),
            TargetLanguage::Dart => format!(
                r#"final __e = {s}FrameEvent("\$>", []);
final __ctx = {s}FrameContext(__e, null);
_context_stack.add(__ctx);
await __kernel(__e);
_context_stack.removeLast();"#,
                s = system_name
            ),
            TargetLanguage::GDScript => format!(
                r#"var __e = {s}FrameEvent.new("$>", [])
var __ctx = {s}FrameContext.new(__e, null)
self._context_stack.append(__ctx)
await self.__kernel(__e)
self._context_stack.pop_back()"#,
                s = system_name
            ),
            TargetLanguage::Kotlin => format!(
                r#"val __e = {s}FrameEvent("$>", mutableListOf<Any?>())
val __ctx = {s}FrameContext(__e, null)
_context_stack.add(__ctx)
__kernel(__e)
_context_stack.removeLast()"#,
                s = system_name
            ),
            TargetLanguage::Swift => format!(
                r#"let __e = {s}FrameEvent(message: "$>", parameters: [])
let __ctx = {s}FrameContext(event: __e)
_context_stack.append(__ctx)
await __kernel(__e)
_context_stack.removeLast()"#,
                s = system_name
            ),
            TargetLanguage::CSharp => format!(
                r#"var __e = new {s}FrameEvent("$>", new List<object>());
var __ctx = new {s}FrameContext(__e, null);
_context_stack.Add(__ctx);
await __kernel(__e);
_context_stack.RemoveAt(_context_stack.Count - 1);"#,
                s = system_name
            ),
            // Languages with async that haven't been implemented yet
            TargetLanguage::Java | TargetLanguage::Go | TargetLanguage::Php => {
                format!("// async init not yet implemented for {:?}", lang)
            }
            // `//` is not a comment in Lua/Ruby — use each language's leader
            // (the old shared placeholder emitted invalid syntax there).
            TargetLanguage::Ruby => format!("# async init not yet implemented for {:?}", lang),
            TargetLanguage::Lua => format!("-- async init not yet implemented for {:?}", lang),
            TargetLanguage::Graphviz => unreachable!(),
        };
        let init_body = vec![CodegenNode::NativeBlock {
            code: init_code,
            span: None,
        }];

        // Swift: `init` is reserved for constructors — `func init() async`
        // is a parse error. Rename just for Swift so tests call
        // `await w.initAsync()` instead of `await w.init()`.
        let init_name = match lang {
            TargetLanguage::Swift => "initAsync".to_string(),
            _ => "init".to_string(),
        };

        methods.push(CodegenNode::Method {
            name: init_name,
            params: vec![],
            return_type: None,
            body: init_body,
            is_async: true,
            is_static: false,
            visibility: Visibility::Public,
            decorators: vec![],
        });
    }
}

/// Ensure a C++ method body contains at least one coroutine keyword
/// (`co_await`, `co_return`, `co_yield`). If none is present, append
/// `co_return;` to the body's last NativeBlock. Required because
/// `FrameTask<T>` is only constructed as a coroutine when the function
/// body contains a coroutine keyword — otherwise the declared return
/// type is a default-constructed `FrameTask<T>` with an empty handle,
/// which crashes on `co_await`.
fn ensure_cpp_coroutine_terminator(body: &mut Vec<CodegenNode>) {
    let has_coroutine_keyword = body.iter().any(|node| {
        if let CodegenNode::NativeBlock { code, .. } = node {
            code.contains("co_await") || code.contains("co_return") || code.contains("co_yield")
        } else {
            false
        }
    });
    if has_coroutine_keyword {
        return;
    }
    if let Some(CodegenNode::NativeBlock { code, .. }) = body
        .iter_mut()
        .rev()
        .find(|n| matches!(n, CodegenNode::NativeBlock { .. }))
    {
        if !code.ends_with('\n') {
            code.push('\n');
        }
        code.push_str("co_return;\n");
    } else {
        body.push(CodegenNode::NativeBlock {
            code: "co_return;\n".to_string(),
            span: None,
        });
    }
}

/// Remove the start-state `$>` kernel dispatch from the constructor body (for
/// async two-phase init — the casing fires it after construction). The dispatch
/// is the sole `FrameInitBlock` in the body (issue #152 marker), so this is an
/// exact structural drop, not a text scan for `__kernel(`.
fn remove_kernel_call_from_body(body: &mut Vec<CodegenNode>) {
    body.retain(|node| !matches!(node, CodegenNode::FrameInitBlock { .. }));
}
