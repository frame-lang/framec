//! Target-specific naming validators.
//!
//! Catches W500/W501-class warnings: GDScript interface methods that would
//! collide with `Object` base-class methods, and TypeScript / JavaScript
//! system names that would shadow web-API / Node.js globals (`Map`, `Worker`,
//! `Buffer`, …).
//!
//! The bottom of the file exposes the two rename-suggestion oracles as free
//! functions so a future code-action / fix-it surface can call them without
//! constructing a `FrameValidator`.

use super::{FrameValidator, ValidationError};
use crate::frame_c::compiler::frame_ast::*;

impl FrameValidator {
    pub fn validate_target_specific(
        &mut self,
        ast: &FrameAst,
        target: crate::frame_c::visitors::TargetLanguage,
    ) -> Result<(), Vec<ValidationError>> {
        // All system names in the compilation unit — E616 checks interface
        // param types against them (a param typed as a sibling system).
        let system_names: std::collections::HashSet<String> = match ast {
            FrameAst::System(system) => std::iter::once(system.name.clone()).collect(),
            FrameAst::Module(module) => module.systems.iter().map(|s| s.name.clone()).collect(),
        };
        match ast {
            FrameAst::System(system) => {
                self.validate_system_target_specific(system, target, &system_names)
            }
            FrameAst::Module(module) => {
                for system in &module.systems {
                    self.validate_system_target_specific(system, target, &system_names);
                }
            }
        }

        if self.errors.is_empty() {
            Ok(())
        } else {
            Err(self.errors.clone())
        }
    }

    pub(super) fn validate_system_target_specific(
        &mut self,
        system: &SystemAst,
        target: crate::frame_c::visitors::TargetLanguage,
        system_names: &std::collections::HashSet<String>,
    ) {
        // E605: Static targets require explicit type on domain fields
        self.validate_domain_types(system, target);
        // E606: Static targets require explicit type on interface params
        self.validate_interface_param_types(system, target);

        // E616 (rust): an interface parameter typed as a live system instance
        // (by value) cannot work — the event is Rc-shared (context stack +
        // kernel), so a move-only payload can never be handed to the handler
        // by value, and cloning a live FSM would be semantically wrong. The
        // reference-semantics equivalent of the OO targets is a shared
        // handle: `Rc<RefCell<Sys>>` (clones by refcount bump, preserves
        // instance identity). #161.
        if matches!(target, crate::frame_c::visitors::TargetLanguage::Rust) {
            // The per-system pass sees one SystemAst; sibling names come from
            // the compilation-wide registry (populated before validation).
            let all_names = crate::frame_c::compiler::codegen::interface_gen::known_system_names();
            for m in &system.interface {
                for p in &m.params {
                    let t = match &p.param_type {
                        crate::frame_c::compiler::frame_ast::Type::Custom(name) => name.trim(),
                        crate::frame_c::compiler::frame_ast::Type::Unknown => continue,
                    };
                    if system_names.contains(t) || all_names.contains(t) {
                        self.errors.push(
                            ValidationError::new(
                                "E616",
                                format!(
                                    "interface method '{}' on system '{}' takes a live system instance by value (`{}: {}`) — on the rust target the event is Rc-shared, so a move-only payload can't reach the handler, and a live state machine must not be cloned. Pass a shared handle instead: `{}: Rc<RefCell<{}>>` (construct with `Rc::new(RefCell::new(@@{}(…)))`; call via `.borrow_mut()`), or construct the child in the domain.",
                                    m.name, system.name, p.name, t, p.name, t, t
                                ),
                            )
                            .with_span(system.span.clone()),
                        );
                    }
                }
            }
        }

        match target {
            crate::frame_c::visitors::TargetLanguage::GDScript => {
                self.validate_gdscript_reserved_methods(system);
            }
            crate::frame_c::visitors::TargetLanguage::Ruby => {
                self.validate_ruby_reserved_methods(system);
            }
            crate::frame_c::visitors::TargetLanguage::TypeScript
            | crate::frame_c::visitors::TargetLanguage::JavaScript => {
                self.validate_typescript_global_collision(system);
            }
            _ => {}
        }
    }

    /// W501: System name shadows a TypeScript / JavaScript global.
    /// The framepiler emits `class <SystemName> { ... }`, so naming a
    /// system `Worker`, `Buffer`, `Promise`, `Map`, etc. produces a
    /// class declaration that shadows the global of the same name in
    /// the surrounding TypeScript scope. This is rarely what the user
    /// wants — every `new Worker(...)` call site in the same file now
    /// instantiates the Frame system instead of the web API.
    ///
    /// Unlike the GDScript case (which is a hard error because the
    /// generated method silently overrides an inherited base-class
    /// method), this is a soft warning: shadowing is legal in
    /// TypeScript and the user may have intentionally chosen the
    /// name. We print the warning to stderr but the build still
    /// succeeds.
    pub(super) fn validate_typescript_global_collision(&mut self, system: &SystemAst) {
        if let Some(suggested) = typescript_global_collision_rename(&system.name) {
            self.warnings.push(
                ValidationError::new(
                    "W501",
                    format!(
                        "System name '{0}' shadows the TypeScript/JavaScript global \
                         `{0}`. The generated `class {0} {{ ... }}` will mask the \
                         built-in within this module — every `new {0}(...)` in the \
                         surrounding code will instantiate the Frame system instead. \
                         Consider renaming (suggested: '{1}'). Pass --no-warnings to \
                         silence.",
                        system.name, suggested
                    ),
                )
                .with_span(system.span.clone()),
            );
        }
    }

    /// E501: Interface method names that would collide with
    /// `Object`'s built-in methods in GDScript. Frame compiles each
    /// interface method to a public method on the generated class,
    /// which inherits from `Object` (via `RefCounted` in Godot 4). If a
    /// user names an interface method `get`, `set`, `call`, etc., the
    /// generated method silently overrides the `Object` method and
    /// every call site that does `obj.get("foo")` ends up routed
    /// through the user's interface method instead. This is a common
    /// foot-gun in the Godot ecosystem; emit a structured error with
    /// a suggested rename rather than letting it surface at runtime.
    pub(super) fn validate_gdscript_reserved_methods(&mut self, system: &SystemAst) {
        for method in &system.interface {
            if let Some(suggested) = gdscript_reserved_method_rename(&method.name) {
                self.errors.push(
                    ValidationError::new(
                        "E501",
                        format!(
                            "Interface method '{}' in system '{}' collides with GDScript's \
                             built-in `Object.{}` method. Calls like `obj.{}(...)` would \
                             silently invoke the Frame interface method instead of the engine \
                             method, breaking core GDScript reflection. Rename it (suggested: '{}').",
                            method.name, system.name, method.name, method.name, suggested
                        ),
                    )
                    .with_span(method.span.clone()),
                );
            }
        }
    }

    /// E501 (Ruby): an interface method whose generated `def <name>` would
    /// override a method Ruby's object model relies on, an unworkable
    /// collision. `initialize` is the constructor — a `def initialize`
    /// event handler silently replaces the system's constructor, breaking
    /// construction. (Contrast `send`, which is *not* rejected: internal
    /// dispatch uses the override-proof `__send__` alias, so a user `send`
    /// event is fine — FRAMEC_BUGS #14.) FRAMEC_BUGS #15.
    pub(super) fn validate_ruby_reserved_methods(&mut self, system: &SystemAst) {
        for method in &system.interface {
            if let Some(suggested) = ruby_reserved_method_rename(&method.name) {
                self.errors.push(
                    ValidationError::new(
                        "E501",
                        format!(
                            "Interface method '{}' in system '{}' collides with Ruby's \
                             `{}` (the object constructor). The generated `def {}` would \
                             silently replace the system's constructor, breaking \
                             construction. Rename it (suggested: '{}').",
                            method.name, system.name, method.name, method.name, suggested
                        ),
                    )
                    .with_span(method.span.clone()),
                );
            }
        }
    }
}

/// Ruby method names an interface method must not use because the
/// generated `def <name>` would override a method Ruby's object model
/// depends on and the collision can't be worked around by the caller.
/// `initialize` is the constructor. (`send` is deliberately absent —
/// internal dispatch uses `__send__`, so a user `send` event is safe.)
/// Returns `None` for names that are safe to use as-is.
pub fn ruby_reserved_method_rename(name: &str) -> Option<&'static str> {
    match name {
        "initialize" => Some("on_initialize"),
        _ => None,
    }
}

/// Godot's `Object` (or close ancestor) class hierarchy. Returns
/// `None` for names that are safe to use as-is.
///
/// The list is intentionally conservative — we only flag names that
/// are documented public methods on `Object` in Godot 4 and that a
/// Frame user might realistically want to name an interface method.
/// We don't flag every internal `_notification`-style helper because
/// underscore-prefixed names are uncommon as interface method names
/// anyway.
///
/// Source: Godot 4 documentation, `Object` class reference.
pub fn gdscript_reserved_method_rename(name: &str) -> Option<&'static str> {
    match name {
        // Property reflection — the most common collisions in practice.
        "get" => Some("get_value"),
        "set" => Some("set_value"),
        // Method reflection / dispatch
        "call" => Some("invoke"),
        "call_deferred" => Some("invoke_deferred"),
        "callv" => Some("invoke_with_args"),
        "has_method" => Some("supports_method"),
        // Lifecycle
        "free" => Some("dispose"),
        "queue_free" => Some("schedule_free"),
        "notification" => Some("notify"),
        // Signals
        "connect" => Some("connect_handler"),
        "disconnect" => Some("disconnect_handler"),
        "emit_signal" => Some("emit"),
        "has_signal" => Some("supports_signal"),
        "get_signal_list" => Some("list_signals"),
        // Class / script reflection
        "get_class" => Some("class_name"),
        "is_class" => Some("is_a"),
        "get_script" => Some("script"),
        "set_script" => Some("attach_script"),
        // Property list
        "get_property_list" => Some("list_properties"),
        // Metadata
        "get_meta" => Some("read_meta"),
        "set_meta" => Some("write_meta"),
        "has_meta" => Some("supports_meta"),
        "remove_meta" => Some("clear_meta"),
        // Stringification / translation
        "to_string" => Some("describe"),
        "tr" => Some("translate"),
        "tr_n" => Some("translate_plural"),
        // Instance identity
        "get_instance_id" => Some("instance_id"),
        // Object lifecycle helpers commonly used in tests
        "is_queued_for_deletion" => Some("is_pending_free"),
        _ => None,
    }
}

/// Global-shadowing check for TypeScript / JavaScript: returns
/// `Some(rename_suggestion)` if `name` would clash with a commonly
/// referenced built-in or web-API global when used as a system name.
/// Returns `None` for names that are safe.
///
/// We focus on names a Frame user might realistically choose for a
/// system class — `Worker` (web/service workers, also a planned
/// framepiler demo), `Buffer` (Node.js), `Map`/`Set`/`Promise` (ES
/// built-ins), `Request`/`Response` (Fetch API), etc. The list is
/// intentionally NOT exhaustive — it covers the high-confidence
/// foot-guns. Esoteric DOM types (`HTMLOListElement` etc.) are
/// excluded to keep the warning signal-to-noise high.
///
/// The suggested rename appends `Sys` so the user can easily
/// disambiguate (`Worker` → `WorkerSys`, `Map` → `MapSys`).
pub fn typescript_global_collision_rename(name: &str) -> Option<String> {
    let is_global = matches!(
        name,
        // Web Workers / Service Workers
        "Worker" | "ServiceWorker" | "SharedWorker" | "WorkerGlobalScope"
        // Node.js core
        | "Buffer" | "Process" | "Console"
        // ES built-in classes
        | "Promise" | "Map" | "Set" | "WeakMap" | "WeakSet"
        | "Date" | "RegExp" | "Error" | "TypeError" | "RangeError" | "SyntaxError"
        | "Array" | "Object" | "String" | "Number" | "Boolean" | "Symbol" | "BigInt"
        | "Function" | "Proxy" | "Reflect"
        | "ArrayBuffer" | "DataView"
        | "Int8Array" | "Uint8Array" | "Uint8ClampedArray"
        | "Int16Array" | "Uint16Array" | "Int32Array" | "Uint32Array"
        | "Float32Array" | "Float64Array" | "BigInt64Array" | "BigUint64Array"
        // DOM / browser globals
        | "Window" | "Document" | "Element" | "Node" | "Event" | "EventTarget"
        | "HTMLElement" | "Image" | "Audio" | "Video"
        | "Storage"
        // Fetch / network
        | "Request" | "Response" | "Headers" | "URL" | "URLSearchParams"
        | "WebSocket" | "XMLHttpRequest" | "FormData"
    );
    if is_global {
        Some(format!("{}Sys", name))
    } else {
        None
    }
}
