//! Shared codegen utilities.
//!
//! Functions and types used across multiple codegen modules:
//! system_codegen, frame_expansion, runtime, erlang_system.

use super::ast::CodegenNode;
use crate::frame_c::compiler::frame_ast::{BinaryOp, Expression, Literal, Type, UnaryOp};
use crate::frame_c::visitors::TargetLanguage;

#[derive(Clone, Default)]
pub(crate) struct HandlerContext {
    pub system_name: String,
    pub state_name: String,
    pub event_name: String,
    pub parent_state: Option<String>,
    /// Set of defined system names in the module (for @@System() validation)
    pub defined_systems: std::collections::HashSet<String>,
    /// True if we're in a state handler that has __sv_comp available for HSM state var access
    pub use_sv_comp: bool,
    /// True when emitting the body of a per-handler method (new architecture
    /// — see docs/frame_runtime.md § "Dispatch Model"). In this mode, the
    /// handler receives a `compartment` parameter that already points at its
    /// own state's compartment (HSM forwards pre-shift it via
    /// `compartment.parent_compartment` at each `=> $^` site). State-var
    /// access therefore emits `compartment.state_vars[…]` directly — no
    /// `__sv_comp` local, no HSM walk preamble. When false, legacy
    /// monolithic dispatch semantics apply (the dispatcher computes
    /// `__sv_comp` once and handler bodies reference it).
    pub per_handler: bool,
    /// Map from state name to its declared HSM parent state name (if any).
    /// Built from `$Child => $Parent` declarations in the machine block.
    /// At transition codegen time, used to eagerly construct the new
    /// compartment's parent chain so `compartment.parent_compartment`
    /// points at the state's declared HSM parent — not the transition-
    /// source compartment. See `_scratch/bug_parent_compartment_hsm_walk.md`.
    pub state_hsm_parents: std::collections::HashMap<String, String>,
    /// State variable types for type-aware expansion (e.g., C++ std::any_cast<Type>)
    pub state_var_types: std::collections::HashMap<String, String>,
    /// Map from state name to its declared param names (in declaration order).
    /// Used by transition codegen to convert positional state args
    /// (`-> $S(42)`) into named writes (`state_args["the_param_name"] = 42`),
    /// matching the named convention used by the state dispatch reader.
    pub state_param_names: std::collections::HashMap<String, Vec<String>>,
    /// Map from state name to its enter handler's declared param names.
    /// Used by transition codegen to convert positional enter args
    /// (`-> "1, 2" $S`) into named writes (`enter_args["the_param_name"] = 1`),
    /// matching the named convention used by enter-handler binding code.
    pub state_enter_param_names: std::collections::HashMap<String, Vec<String>>,
    /// Map from state name to its exit handler's declared param names.
    /// Used by transition codegen to convert positional exit args
    /// (`("a", b) -> $S`) into named writes
    /// (`exit_args["the_param_name"] = ...`), matching the named
    /// convention the dispatch reader uses for exit handlers.
    pub state_exit_param_names: std::collections::HashMap<String, Vec<String>>,
    /// Map from event name to its interface method's declared param names.
    /// Used by @@:params.name to resolve named parameter to positional index.
    pub event_param_names: std::collections::HashMap<String, Vec<String>>,
    /// (state_name, param_name) → declared type string. Populated for the
    /// effective view: a state's own params plus any params declared at
    /// a descendant's cascade arrow `$Child => $Self(name: T)`. Used by
    /// typed-language per-handler emit so the prefetch cast/declaration
    /// matches the declared type instead of defaulting to `int`.
    pub state_param_types: std::collections::HashMap<(String, String), String>,
    /// (state_name, param_name) → declared type of the state's `$>` enter
    /// handler params. Used by the C transition codegen (#81) to pack each
    /// enter arg per its declared marshal category (float/double heap-box
    /// via pack_double, pushed owned). Empty on backends that don't need
    /// write-side categorization (erased containers carry the type).
    pub state_enter_param_types: std::collections::HashMap<(String, String), String>,
    /// (state_name, param_name) → declared type of the state's `<$` exit
    /// handler params. C transition codegen write-side mirror of
    /// `state_enter_param_types` (#81); keyed by the transition's SOURCE
    /// state (the one being exited).
    pub state_exit_param_types: std::collections::HashMap<(String, String), String>,
    /// Declared return type of the handler currently being expanded.
    /// Used by the C backend to branch on `float`/`double` when emitting
    /// `@@:(expr)` so doubles survive the `void*` return slot.
    pub current_return_type: Option<String>,
    /// Domain field name → declared type (clean, e.g. `Ship`). Used by
    /// `@@:self.field.method()` (RFC-0046) to decide whether `field` is an
    /// embedded system (type ∈ `defined_systems` → cross-system call /
    /// pointer deref) or a scalar (native value method). Empty where the
    /// info is unavailable (no embed calls expected there).
    pub domain_field_types: std::collections::HashMap<String, String>,
    /// The system's action names (RFC-0046). `@@:self.<action>(args)` is a
    /// *direct* call, not a kernel-dispatched interface call, so it must NOT
    /// receive the caller-side transition guard. The body walk consults this
    /// to suppress the guard for action calls.
    pub actions: std::collections::HashSet<String>,
    /// RFC-0043 / #158: true when the system is `@@[async]`-layered. Consulted
    /// at EMISSION so dispatch calls carry their `await`/`co_await`/`.await`
    /// directly, instead of a post-pass rescanning emitted text.
    pub system_is_async: bool,
}

/// Coerce a value expression to its DECLARED float-family type at the point
/// it enters a target's type-erasure layer (#77).
///
/// The type-erased backends store state-vars / the return slot in an erased
/// container (`std::any`, `object`, `Object`, `Any`, `interface{}`) and read
/// back with an EXACT-match cast of the declared type. A bare literal,
/// however, deduces/boxes to the target's default float width (`0.0` →
/// C++ `double`, Java `Double`, Go `float64`, …), so a `float`-declared slot
/// stores a double and every read crashes at runtime (`std::bad_any_cast` /
/// `InvalidCastException` / `ClassCastException` / `as!` trap / interface
/// panic) — while compiling cleanly. Third sighting of the typed-erasure
/// round-trip class (#59 Rust literals, #72 C `void*`, #77 this).
///
/// The discipline: every WRITE into an erased slot coerces to the declared
/// type; reads stay exact. Coercion is a no-op when the expression already
/// has the declared type, so it is safe to apply unconditionally at the
/// write chokepoints. Only the float family needs it (the only case where a
/// valid literal's deduced type differs from a declared native type);
/// everything else returns the expression unchanged. C is handled separately
/// via `c_marshal` (bit-pun pack/unpack — there is no typed box to match).
pub(crate) fn erased_write_coercion(lang: TargetLanguage, declared: &str, expr: &str) -> String {
    let t = declared.trim();
    match lang {
        TargetLanguage::Cpp if t == "float" || t == "double" => format!("({t})({expr})"),
        TargetLanguage::CSharp if t == "float" || t == "double" => format!("({t})({expr})"),
        TargetLanguage::Java if t == "float" || t == "double" => format!("({t})({expr})"),
        TargetLanguage::Kotlin if t == "Float" => format!("({expr}).toFloat()"),
        TargetLanguage::Kotlin if t == "Double" => format!("({expr}).toDouble()"),
        TargetLanguage::Swift if t == "Float" || t == "Double" => format!("{t}({expr})"),
        TargetLanguage::Go if t == "float32" || t == "float64" => format!("{t}({expr})"),
        _ => expr.to_string(),
    }
}

/// The state-var initializer, verbatim.
///
/// E610 rejects state-vars without an initializer before codegen runs, so
/// the text is always present here. Frame does not synthesize default
/// values — the per-target default table that used to live here picked a
/// value by lowercase-matching the TYPE NAME, the alias-table species the
/// 4.5.0 verbatim-passthrough release exterminated (issue #84).
pub(crate) fn state_var_initializer(
    var: &crate::frame_c::compiler::frame_ast::StateVarAst,
) -> String {
    var.initializer_text.clone().unwrap_or_else(|| {
        unreachable!(
            "state-var '$.{}' has no initializer — E610 must reject this before codegen",
            var.name
        )
    })
}

// `typed_init_expr` (removed): previously wrapped portable state-var init
// expressions per target (`""` -> `String::from("")` for Rust, etc.). That
// contradicted the verbatim-passthrough contract (docs/frame_language.md) and
// was the parse-and-re-serialize path that corrupted literals like `0.0` ->
// `0` (FRAMEC #59). State-var initializers now carry raw text
// (`StateVarAst::initializer_text`) and are emitted verbatim, exactly like
// domain-field initializers — the user writes their target's native init value.

/// Convert an Expression to a string representation for inline code
pub(crate) fn expression_to_string(expr: &Expression, lang: TargetLanguage) -> String {
    match expr {
        Expression::Literal(lit) => match lit {
            Literal::Int(n) => n.to_string(),
            Literal::Float(f) => {
                // `f64::to_string()` drops the decimal for whole numbers
                // (`0.0` -> `"0"`), which emits an integer literal into a
                // float slot — uncompilable on typed targets (FRAMEC #59).
                // Re-add it only for the pure-integer rendering; leave
                // `1.5` / `1e10` / `inf` / `NaN` untouched.
                let s = f.to_string();
                if s.bytes().all(|b| b.is_ascii_digit() || b == b'-') {
                    format!("{s}.0")
                } else {
                    s
                }
            }
            Literal::String(s) => format!("\"{}\"", s),
            Literal::Bool(b) => match lang {
                TargetLanguage::Python3 => {
                    if *b {
                        "True".to_string()
                    } else {
                        "False".to_string()
                    }
                }
                TargetLanguage::GDScript
                | TargetLanguage::Dart
                | TargetLanguage::TypeScript
                | TargetLanguage::JavaScript
                | TargetLanguage::Rust
                | TargetLanguage::C
                | TargetLanguage::Cpp
                | TargetLanguage::Java
                | TargetLanguage::Kotlin
                | TargetLanguage::Swift
                | TargetLanguage::CSharp
                | TargetLanguage::Go
                | TargetLanguage::Php
                | TargetLanguage::Ruby
                | TargetLanguage::Lua => {
                    if *b {
                        "true".to_string()
                    } else {
                        "false".to_string()
                    }
                }
                TargetLanguage::Graphviz => unreachable!(),
            },
            Literal::Null => match lang {
                TargetLanguage::Python3 | TargetLanguage::Rust => "None".to_string(),
                TargetLanguage::Cpp => "nullptr".to_string(),
                TargetLanguage::Go
                | TargetLanguage::Swift
                | TargetLanguage::Ruby
                | TargetLanguage::Lua => "nil".to_string(),
                TargetLanguage::C => "NULL".to_string(),
                TargetLanguage::GDScript
                | TargetLanguage::Dart
                | TargetLanguage::TypeScript
                | TargetLanguage::JavaScript
                | TargetLanguage::Java
                | TargetLanguage::Kotlin
                | TargetLanguage::CSharp
                | TargetLanguage::Php => "null".to_string(),
                TargetLanguage::Graphviz => unreachable!(),
            },
        },
        Expression::Var(name) => name.clone(),
        Expression::Binary { left, op, right } => {
            let op_str = match op {
                BinaryOp::Add => "+",
                BinaryOp::Sub => "-",
                BinaryOp::Mul => "*",
                BinaryOp::Div => "/",
                BinaryOp::Mod => "%",
                BinaryOp::Eq => "==",
                BinaryOp::Ne => "!=",
                BinaryOp::Lt => "<",
                BinaryOp::Le => "<=",
                BinaryOp::Gt => ">",
                BinaryOp::Ge => ">=",
                BinaryOp::And => "&&",
                BinaryOp::Or => "||",
                BinaryOp::BitAnd => "&",
                BinaryOp::BitOr => "|",
                BinaryOp::BitXor => "^",
            };
            format!(
                "{} {} {}",
                expression_to_string(left, lang),
                op_str,
                expression_to_string(right, lang)
            )
        }
        Expression::Unary { op, expr } => {
            let op_str = match op {
                UnaryOp::Not => "!",
                UnaryOp::Neg => "-",
                UnaryOp::BitNot => "~",
            };
            format!("{}{}", op_str, expression_to_string(expr, lang))
        }
        // NativeExpr carries the source verbatim — used for target-specific
        // literals (list/map/closure forms) that the Frame parser doesn't
        // structurally model. State-var inits use this path for things like
        // `$.items: list = [1, 2, 3]` — the bytes between `=` and the next
        // separator are captured as-is and emitted unchanged. The previous
        // fallback silently substituted `"0"` for anything non-primitive,
        // which turned `[1, 2, 3]` into an int and looked like a typo in
        // the generated code.
        Expression::NativeExpr(code) => code.clone(),
        _ => "0".to_string(), // Fallback for complex expressions (Call/Member/Index/Assign)
    }
}

/// Convert Type enum to string representation
pub(crate) fn type_to_string(t: &Type) -> String {
    match t {
        Type::Custom(name) => name.clone(),
        Type::Unknown => "Any".to_string(),
    }
}

/// Convert Expression AST to CodegenNode
pub(crate) fn convert_expression(expr: &Expression) -> CodegenNode {
    match expr {
        Expression::Var(name) => CodegenNode::ident(name),
        Expression::Literal(lit) => convert_literal(lit),
        Expression::Binary { left, op, right } => {
            let codegen_op = match op {
                BinaryOp::Add => crate::frame_c::compiler::codegen::ast::BinaryOp::Add,
                BinaryOp::Sub => crate::frame_c::compiler::codegen::ast::BinaryOp::Sub,
                BinaryOp::Mul => crate::frame_c::compiler::codegen::ast::BinaryOp::Mul,
                BinaryOp::Div => crate::frame_c::compiler::codegen::ast::BinaryOp::Div,
                BinaryOp::Mod => crate::frame_c::compiler::codegen::ast::BinaryOp::Mod,
                BinaryOp::Eq => crate::frame_c::compiler::codegen::ast::BinaryOp::Eq,
                BinaryOp::Ne => crate::frame_c::compiler::codegen::ast::BinaryOp::Ne,
                BinaryOp::Lt => crate::frame_c::compiler::codegen::ast::BinaryOp::Lt,
                BinaryOp::Le => crate::frame_c::compiler::codegen::ast::BinaryOp::Le,
                BinaryOp::Gt => crate::frame_c::compiler::codegen::ast::BinaryOp::Gt,
                BinaryOp::Ge => crate::frame_c::compiler::codegen::ast::BinaryOp::Ge,
                BinaryOp::And => crate::frame_c::compiler::codegen::ast::BinaryOp::And,
                BinaryOp::Or => crate::frame_c::compiler::codegen::ast::BinaryOp::Or,
                BinaryOp::BitAnd => crate::frame_c::compiler::codegen::ast::BinaryOp::BitAnd,
                BinaryOp::BitOr => crate::frame_c::compiler::codegen::ast::BinaryOp::BitOr,
                BinaryOp::BitXor => crate::frame_c::compiler::codegen::ast::BinaryOp::BitXor,
            };
            CodegenNode::BinaryOp {
                op: codegen_op,
                left: Box::new(convert_expression(left)),
                right: Box::new(convert_expression(right)),
            }
        }
        Expression::Unary { op, expr } => {
            let codegen_op = match op {
                UnaryOp::Neg => crate::frame_c::compiler::codegen::ast::UnaryOp::Neg,
                UnaryOp::Not => crate::frame_c::compiler::codegen::ast::UnaryOp::Not,
                UnaryOp::BitNot => crate::frame_c::compiler::codegen::ast::UnaryOp::BitNot,
            };
            CodegenNode::UnaryOp {
                op: codegen_op,
                operand: Box::new(convert_expression(expr)),
            }
        }
        Expression::Call { func, args } => CodegenNode::Call {
            target: Box::new(CodegenNode::ident(func)),
            args: args.iter().map(convert_expression).collect(),
        },
        Expression::Index { object, index } => CodegenNode::IndexAccess {
            object: Box::new(convert_expression(object)),
            index: Box::new(convert_expression(index)),
        },
        Expression::Member { object, field } => CodegenNode::FieldAccess {
            object: Box::new(convert_expression(object)),
            field: field.clone(),
        },
        Expression::Assign { target, value } => {
            CodegenNode::assign(convert_expression(target), convert_expression(value))
        }
        Expression::NativeExpr(code) => {
            // Pass through native expression verbatim
            CodegenNode::native(code)
        }
    }
}

/// Convert Literal to CodegenNode
pub(crate) fn convert_literal(lit: &Literal) -> CodegenNode {
    match lit {
        Literal::Int(n) => CodegenNode::int(*n),
        Literal::Float(f) => CodegenNode::float(*f),
        Literal::String(s) => CodegenNode::string(s),
        Literal::Bool(b) => CodegenNode::bool(*b),
        Literal::Null => CodegenNode::null(),
    }
}

/// Map a Frame type string to C# type for (Type) cast.
///
/// RFC-0035 round 2: Frame-implemented in `compiler/type_map/`.
pub(crate) fn csharp_map_type(t: &str) -> String {
    crate::frame_c::compiler::type_map::csharp_map_type(t)
}

/// Map a Frame type string to Java type for (Type) cast.
///
/// RFC-0035 round 2: Frame-implemented in `compiler/type_map/`.
pub(crate) fn java_map_type(t: &str) -> String {
    crate::frame_c::compiler::type_map::java_map_type(t)
}

/// Map a Frame type string to Kotlin type for cast.
///
/// RFC-0035 round 2: Frame-implemented in `compiler/type_map/`.
pub(crate) fn kotlin_map_type(t: &str) -> String {
    crate::frame_c::compiler::type_map::kotlin_map_type(t)
}

/// Map a Frame type string to Swift type for cast. Handles
/// `T | nil` → `T?` and `T[]` → `[T]` recursively.
///
/// RFC-0035 round 2: Frame-implemented in `compiler/type_map/`.
pub(crate) fn swift_map_type(t: &str) -> String {
    crate::frame_c::compiler::type_map::swift_map_type(t)
}

/// Map a Frame type string to Go type for type assertion.
///
/// RFC-0035 round 2: Frame-implemented in `compiler/type_map/`.
pub(crate) fn go_map_type(t: &str) -> String {
    crate::frame_c::compiler::type_map::go_map_type(t)
}

/// Map a Frame type string to C++ type for std::any_cast<T>.
///
/// RFC-0035 round 2: Frame-implemented in `compiler/type_map/`.
pub(crate) fn cpp_map_type(t: &str) -> String {
    crate::frame_c::compiler::type_map::cpp_map_type(t)
}

/// Wrap a C++ argument value for std::any storage.
/// String literals ("...") must be wrapped in std::string() because
/// std::any("literal") stores const char*, not std::string.
pub(crate) fn cpp_wrap_any_arg(arg: &str) -> String {
    let trimmed = arg.trim();
    if trimmed.starts_with('"') && trimmed.ends_with('"') {
        format!("std::string({})", trimmed)
    } else {
        trimmed.to_string()
    }
}

/// Convert Frame Type to C++ type string
pub(crate) fn type_to_cpp_string(t: &crate::frame_c::compiler::frame_ast::Type) -> String {
    match t {
        crate::frame_c::compiler::frame_ast::Type::Unknown => "void".to_string(),
        crate::frame_c::compiler::frame_ast::Type::Custom(s) => match s.as_str() {
            "str" | "string" | "String" => "std::string".to_string(),
            "int" | "i32" | "i64" => "int".to_string(),
            "float" | "f64" | "f32" => "double".to_string(),
            "bool" => "bool".to_string(),
            "void" => "void".to_string(),
            other => other.to_string(),
        },
    }
}

/// Convert CamelCase / PascalCase to snake_case.
///
/// Delegates to the Frame-implemented FSM in
/// `framec::frame_c::compiler::name::to_snake_case` (RFC-0035
/// round 1). The implementation lives in
/// `framec/src/frame_c/compiler/name/to_snake_case.frs`.
pub(crate) fn to_snake_case(s: &str) -> String {
    crate::frame_c::compiler::name::to_snake_case(s)
}

/// Uppercase the first character (rest verbatim). This is the Go
/// backend's export rename for public method names (`tick` → `Tick`,
/// `get_lives` → `Get_lives`). It is shared so the method *definition*
/// (go.rs) and a cross-system *call site* (`@@:self.field.method()`,
/// `context_self.rs`) use the identical mapping and can't drift (#112).
pub(crate) fn capitalize_first(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        None => String::new(),
        Some(c) => c.to_uppercase().collect::<String>() + chars.as_str(),
    }
}

/// Backtick-escape a Swift reserved keyword used as an identifier (#175).
///
/// Frame method, field, and parameter names are user-authored and can collide
/// with Swift keywords — `init`, `default`, `guard`, `case`, `deinit`,
/// `subscript`, … — all valid Frame identifiers. Swift is the only one of the
/// 17 targets that rejects such an identifier unless it is backtick-escaped
/// (`` func `init` ``), at both the declaration and every call site. This wraps
/// a name in backticks iff it is a reserved word; every other name (including
/// the generated framework identifiers `_state_S`, `__transition`, `__e`, …)
/// passes through unchanged. Apply it only where a Frame name is emitted as a
/// Swift *identifier* — never to the string dispatch keys (`message == "init"`),
/// which are plain string literals.
///
/// Single source of truth: used by both the Swift backend (structural
/// interface/method emission) and the `@@:self.*` self-call lowering in
/// `frame_expansion`.
pub(crate) fn swift_escape_ident(name: &str) -> std::borrow::Cow<'_, str> {
    // Swift reserved words that are invalid as a bare identifier. `self`,
    // `Self`, and `super` are intentionally omitted: they carry special
    // semantics even when backtick-escaped and are emitted structurally, never
    // as a user name.
    const SWIFT_KEYWORDS: &[&str] = &[
        // declarations
        "associatedtype",
        "class",
        "deinit",
        "enum",
        "extension",
        "fileprivate",
        "func",
        "import",
        "init",
        "inout",
        "internal",
        "let",
        "open",
        "operator",
        "private",
        "protocol",
        "public",
        "rethrows",
        "static",
        "struct",
        "subscript",
        "typealias",
        "var",
        // statements
        "break",
        "case",
        "continue",
        "default",
        "defer",
        "do",
        "else",
        "fallthrough",
        "for",
        "guard",
        "if",
        "in",
        "repeat",
        "return",
        "switch",
        "where",
        "while",
        // expressions & types
        "as",
        "await",
        "catch",
        "false",
        "is",
        "nil",
        "throw",
        "throws",
        "true",
        "try",
        "Any",
    ];
    if SWIFT_KEYWORDS.contains(&name) {
        std::borrow::Cow::Owned(format!("`{}`", name))
    } else {
        std::borrow::Cow::Borrowed(name)
    }
}

/// Backtick-escape bare references to Swift-keyword-named params in lowered
/// native handler text (#175).
///
/// Companion to [`swift_escape_ident`]: a param whose name is a Swift keyword
/// (`default`, `guard`, …) has its binding emitted as `let `default` = …`, so
/// its *references* in the handler body must be escaped to match, or `swiftc`
/// rejects the keyword. Mirrors the PHP `php_prefix_params` native-rewrite
/// pattern — boundary-safe (skips string literals, comments, and spans already
/// inside backticks) and word-boundary-exact.
///
/// Only names in `params` that are *also* Swift keywords are touched; every
/// other identifier passes through, so this is a fast no-op for the
/// overwhelmingly common case of non-keyword param names.
pub(crate) fn swift_escape_keyword_param_refs(text: &str, params: &[String]) -> String {
    // Nothing to do unless some param name is actually a Swift keyword.
    let keyword_params: Vec<&str> = params
        .iter()
        .filter(|p| matches!(swift_escape_ident(p), std::borrow::Cow::Owned(_)))
        .map(|p| p.as_str())
        .collect();
    if keyword_params.is_empty() {
        return text.to_string();
    }
    let is_ident_char = |b: u8| b.is_ascii_alphanumeric() || b == b'_';
    let skipper = crate::frame_c::compiler::native_region_scanner::create_skipper(
        crate::frame_c::visitors::TargetLanguage::Swift,
    );
    let bytes = text.as_bytes();
    let end = bytes.len();
    let mut out = String::with_capacity(text.len());
    let mut i = 0;
    while i < end {
        // String literals and comments pass through verbatim.
        if let Some(next) = skipper.skip_string(bytes, i, end) {
            out.push_str(&text[i..next]);
            i = next;
            continue;
        }
        if let Some(next) = skipper.skip_comment(bytes, i, end) {
            out.push_str(&text[i..next]);
            i = next;
            continue;
        }
        // An already-escaped `...` identifier passes through verbatim (avoids
        // double-escaping a field/method name a prior pass already wrapped).
        if bytes[i] == b'`' {
            let start = i;
            i += 1;
            while i < end && bytes[i] != b'`' {
                i += 1;
            }
            if i < end {
                i += 1; // consume the closing backtick
            }
            out.push_str(&text[start..i]);
            continue;
        }
        let c = bytes[i];
        let is_ident_start =
            (c.is_ascii_alphabetic() || c == b'_') && !(i > 0 && is_ident_char(bytes[i - 1]));
        if is_ident_start {
            let start = i;
            while i < end && is_ident_char(bytes[i]) {
                i += 1;
            }
            let ident = &text[start..i];
            if keyword_params.contains(&ident) {
                out.push('`');
                out.push_str(ident);
                out.push('`');
            } else {
                out.push_str(ident);
            }
            continue;
        }
        out.push(c as char);
        i += 1;
    }
    out
}

// ─── String-aware literal replace ────────────────────────────────────
//
// Used by codegen branches that need to rewrite tokens inside generated
// native code (e.g. `self.` → `s.` for Go, `self.` → `Data#data.` for
// Erlang). A naive `str::replace` would false-match inside string and
// comment literals; this walker delegates string/comment skipping to
// the target language's `SyntaxSkipper` and only performs replacements
// in code positions.

/// Replace each `(needle, replacement)` pair in `code`, skipping over
/// string literals and comments using the language's `SyntaxSkipper`.
///
/// Matches are literal substrings (not regex); the first-matching rule
/// at each byte position wins, so order rules with longer / more-
/// specific needles first if overlapping.
///
/// Replacement is safe against:
///   - single- and double-quoted string contents (per language rules)
///   - single-line and block comments (per language rules)
///   - backslash escapes inside strings (Rust/JS/etc.)
///
/// This is the preferred alternative to `str::replace` in any codegen
/// branch operating on already-emitted native code.
pub fn replace_outside_strings_and_comments(
    code: &str,
    lang: crate::frame_c::visitors::TargetLanguage,
    replacements: &[(&str, &str)],
) -> String {
    let skipper = crate::frame_c::compiler::native_region_scanner::create_skipper(lang);
    let bytes = code.as_bytes();
    let end = bytes.len();
    let mut out = String::with_capacity(code.len());
    let mut i = 0;
    while i < end {
        // Delegate string-literal skipping to the language skipper.
        if let Some(next) = skipper.skip_string(bytes, i, end) {
            out.push_str(&code[i..next]);
            i = next;
            continue;
        }
        // Delegate comment skipping.
        if let Some(next) = skipper.skip_comment(bytes, i, end) {
            out.push_str(&code[i..next]);
            i = next;
            continue;
        }
        // Try each replacement rule; first match wins.
        let mut replaced = false;
        for (needle, replacement) in replacements {
            let nb = needle.as_bytes();
            if i + nb.len() <= end && &bytes[i..i + nb.len()] == nb {
                out.push_str(replacement);
                i += nb.len();
                replaced = true;
                break;
            }
        }
        if replaced {
            continue;
        }
        // Plain character — copy through. Advance by the full UTF-8
        // width so a multibyte sequence in an identifier or unquoted
        // literal isn't split across push calls.
        let width = utf8_char_len(bytes[i]);
        let next = (i + width).min(end);
        out.push_str(&code[i..next]);
        i = next;
    }
    out
}

/// Find the first occurrence of `needle` in `code` that lies **outside** string
/// literals and comments, per the language's `SyntaxSkipper`. Returns its byte
/// offset, or `None`. The string/comment-safe counterpart of `str::find` — use
/// it before scanning a sigil (`@@:return(`, `@@:(`, …) in a native body so a
/// sigil embedded in a literal or comment is not treated as real syntax.
pub fn find_outside_strings_and_comments(
    code: &str,
    lang: crate::frame_c::visitors::TargetLanguage,
    needle: &str,
) -> Option<usize> {
    find_outside_strings_and_comments_from(code, lang, needle, 0)
}

/// As [`find_outside_strings_and_comments`], but begins scanning at byte offset
/// `start`. `start` MUST be a code-region position (not inside a string literal
/// or comment) — callers walking match-by-match satisfy this by resuming just
/// past a previous code-region match.
pub fn find_outside_strings_and_comments_from(
    code: &str,
    lang: crate::frame_c::visitors::TargetLanguage,
    needle: &str,
    start: usize,
) -> Option<usize> {
    let nb = needle.as_bytes();
    if nb.is_empty() {
        return None;
    }
    let skipper = crate::frame_c::compiler::native_region_scanner::create_skipper(lang);
    let bytes = code.as_bytes();
    let end = bytes.len();
    let mut i = start.min(end);
    while i < end {
        if let Some(next) = skipper.skip_string(bytes, i, end) {
            i = next;
            continue;
        }
        if let Some(next) = skipper.skip_comment(bytes, i, end) {
            i = next;
            continue;
        }
        if bytes[i..].starts_with(nb) {
            return Some(i);
        }
        i += 1;
    }
    None
}

/// Like [`replace_outside_strings_and_comments`], but a replacement only fires
/// when its needle sits at a **left word boundary** — the byte immediately
/// before it is not a word character (alphanumeric or `_`). This is the safe
/// primitive for rewriting a *receiver prefix* on already-emitted code — e.g.
/// `s.` → `c.` (Go) or `self.` → `c.` (Rust) when relocating a bare-constructor
/// body into the factory — without touching a longer identifier that merely ends
/// in the receiver (`sensors.`, `myself.`) or the token inside a string/comment.
///
/// Prefer this over a raw `str::replace` or a hand-rolled byte scan for any
/// receiver/identifier-prefix rewrite of emitted native code.
pub fn replace_word_start_outside_strings_and_comments(
    code: &str,
    lang: crate::frame_c::visitors::TargetLanguage,
    replacements: &[(&str, &str)],
) -> String {
    let skipper = crate::frame_c::compiler::native_region_scanner::create_skipper(lang);
    let bytes = code.as_bytes();
    let end = bytes.len();
    let mut out = String::with_capacity(code.len());
    let mut i = 0;
    while i < end {
        if let Some(next) = skipper.skip_string(bytes, i, end) {
            out.push_str(&code[i..next]);
            i = next;
            continue;
        }
        if let Some(next) = skipper.skip_comment(bytes, i, end) {
            out.push_str(&code[i..next]);
            i = next;
            continue;
        }
        let prev_is_word = i > 0 && (bytes[i - 1].is_ascii_alphanumeric() || bytes[i - 1] == b'_');
        if !prev_is_word {
            let mut replaced = false;
            for (needle, replacement) in replacements {
                let nb = needle.as_bytes();
                if i + nb.len() <= end && &bytes[i..i + nb.len()] == nb {
                    out.push_str(replacement);
                    i += nb.len();
                    replaced = true;
                    break;
                }
            }
            if replaced {
                continue;
            }
        }
        let width = utf8_char_len(bytes[i]);
        let next = (i + width).min(end);
        out.push_str(&code[i..next]);
        i = next;
    }
    out
}

/// Split a comma-separated argument list at **top-level commas only** — a comma
/// nested inside `()`/`[]`/`{}`, or inside a string literal / comment, is not a
/// split point. String/comment skipping is delegated to the language's
/// `SyntaxSkipper`. Each returned arg is trimmed; empty/whitespace input → `[]`.
///
/// This is the correct primitive for any codegen operating per-argument on an
/// emitted or captured arg blob (transition args #148, nested self-call hoisting
/// #150), replacing a naive `str::split(',')` that breaks on `f(a, b)` or a
/// comma inside a `"a,b"` literal.
/// True iff a domain-field initializer is a **runtime expression** — one that
/// evaluates by constructing an object, calling a function/method, or
/// instantiating a sibling system — as opposed to a compile-time *constant
/// expression* (literal, arithmetic/array/dict of constants, a constant/enum
/// reference). A runtime expression cannot be a static field default in most
/// targets and must be assigned in the constructor/init path.
///
/// This is the token-based classifier that replaces the `.contains("@@") ||
/// .contains("new ") || .contains('(')` substring heuristic (#153 → #144). It
/// walks the init text via the language `SyntaxSkipper` and reports runtime iff,
/// **outside string literals and comments**, it finds any of:
///   - `@@` — a `@@<System>()` instantiation;
///   - the `new` keyword at an identifier boundary;
///   - an identifier immediately (modulo spaces) followed by `(` — a call.
///
/// So `(1 + 2)` (parenthesised constant) is NOT runtime — fixing the old
/// heuristic's `contains('(')` false-positive — while `f(1, 2)`, `new X()`,
/// `Vec2(1,2)`, and `"a" . new Y()` (concat) are. A `(` or `new`/`@@` inside a
/// string literal is ignored, so no whole-string-literal special case is needed.
pub fn init_is_runtime_expression(
    init_text: &str,
    lang: crate::frame_c::visitors::TargetLanguage,
) -> bool {
    let t = init_text.trim();
    if t.is_empty() {
        return false;
    }
    let skipper = crate::frame_c::compiler::native_region_scanner::create_skipper(lang);
    let bytes = t.as_bytes();
    let end = bytes.len();
    let is_word = |b: u8| b.is_ascii_alphanumeric() || b == b'_';
    let mut i = 0usize;
    while i < end {
        if let Some(next) = skipper.skip_string(bytes, i, end) {
            i = next;
            continue;
        }
        if let Some(next) = skipper.skip_comment(bytes, i, end) {
            i = next;
            continue;
        }
        // `@@<System>()` instantiation.
        if bytes[i..].starts_with(b"@@") {
            return true;
        }
        // `new` keyword at an identifier boundary.
        if bytes[i..].starts_with(b"new") {
            let before_ok = i == 0 || !is_word(bytes[i - 1]);
            let after = i + 3;
            let after_ok = after >= end || !is_word(bytes[after]);
            if before_ok && after_ok {
                return true;
            }
        }
        // An identifier immediately (skipping spaces) before a `(` is a call.
        if bytes[i] == b'(' {
            let mut k = i;
            while k > 0 && (bytes[k - 1] == b' ' || bytes[k - 1] == b'\t') {
                k -= 1;
            }
            if k > 0 && is_word(bytes[k - 1]) {
                return true;
            }
        }
        let w = utf8_char_len(bytes[i]).max(1);
        i += w;
    }
    false
}

pub fn split_top_level_args(
    args: &str,
    lang: crate::frame_c::visitors::TargetLanguage,
) -> Vec<String> {
    if args.trim().is_empty() {
        return Vec::new();
    }
    let skipper = crate::frame_c::compiler::native_region_scanner::create_skipper(lang);
    let bytes = args.as_bytes();
    let end = bytes.len();
    let mut out = Vec::new();
    let mut depth: i32 = 0;
    let mut start = 0usize;
    let mut i = 0usize;
    while i < end {
        if let Some(next) = skipper.skip_string(bytes, i, end) {
            i = next;
            continue;
        }
        if let Some(next) = skipper.skip_comment(bytes, i, end) {
            i = next;
            continue;
        }
        match bytes[i] {
            b'(' | b'[' | b'{' => depth += 1,
            b')' | b']' | b'}' => depth -= 1,
            b',' if depth == 0 => {
                out.push(args[start..i].trim().to_string());
                start = i + 1;
            }
            _ => {}
        }
        i += 1;
    }
    out.push(args[start..end].trim().to_string());
    out
}

/// Byte index of the depth-0 `)` that closes an open paren whose contents
/// start at `from` (i.e. `from` is the byte just past the `(`). String- and
/// comment-aware via the language skipper, and bracket-depth-aware for all of
/// `()[]{}` — a `)` inside a string literal or a nested pair does not close.
/// Returns `None` if the parens are unbalanced (no depth-0 `)` before `end`).
/// #123/#154: the single matching-close primitive for codegen — replaces
/// hand-rolled, string-blind depth walks.
pub(crate) fn matching_close_paren(
    code: &str,
    lang: crate::frame_c::visitors::TargetLanguage,
    from: usize,
) -> Option<usize> {
    let skipper = crate::frame_c::compiler::native_region_scanner::create_skipper(lang);
    let bytes = code.as_bytes();
    let end = bytes.len();
    let mut depth: i32 = 0;
    let mut i = from;
    while i < end {
        if let Some(next) = skipper.skip_string(bytes, i, end) {
            i = next;
            continue;
        }
        if let Some(next) = skipper.skip_comment(bytes, i, end) {
            i = next;
            continue;
        }
        match bytes[i] {
            b'(' | b'[' | b'{' => depth += 1,
            b')' if depth == 0 => return Some(i),
            b')' | b']' | b'}' => depth -= 1,
            _ => {}
        }
        i += 1;
    }
    None
}

/// Split a transition argument blob into the list of argument **values**, with
/// any `name = ` prefix stripped from a named argument. Splitting is at
/// top-level commas only (via [`split_top_level_args`]) and the `name =` strip
/// is depth-aware and ignores `==`/`<=`/… so a comparison or a nested `=` is not
/// mistaken for a separator. #148: replaces the old
/// `blob.split(',').map(..).map(|a| a.find('='))` which broke on a comma or `=`
/// nested inside a value (`$S(makeList(a, b))`, `$S(x = point(1, 2))`).
pub fn arg_values(blob: &str, lang: crate::frame_c::visitors::TargetLanguage) -> Vec<String> {
    split_top_level_args(blob, lang)
        .iter()
        .map(|a| strip_named_arg(a))
        .collect()
}

/// If a single argument is a `name = value` form, return `Some((name, value))`;
/// otherwise `None`. The separator is the first top-level `=` whose LHS is a
/// bare identifier and which is not part of `==`/`!=`/`<=`/`>=` — so a
/// comparison, a nested `f(a=b)`, or a string value is not mistaken for a
/// named-arg separator.
pub(crate) fn split_named_arg(arg: &str) -> Option<(String, String)> {
    let t = arg.trim();
    let bytes = t.as_bytes();
    let mut depth = 0i32;
    let mut i = 0usize;
    while i < bytes.len() {
        match bytes[i] {
            b'(' | b'[' | b'{' => depth += 1,
            b')' | b']' | b'}' => depth -= 1,
            b'=' if depth == 0 => {
                let next_eq = i + 1 < bytes.len() && bytes[i + 1] == b'=';
                let prev_op = i > 0 && matches!(bytes[i - 1], b'=' | b'!' | b'<' | b'>');
                if !next_eq && !prev_op {
                    let lhs = t[..i].trim();
                    if !lhs.is_empty()
                        && lhs.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
                    {
                        return Some((lhs.to_string(), t[i + 1..].trim().to_string()));
                    }
                    return None;
                }
            }
            _ => {}
        }
        i += 1;
    }
    None
}

/// From a single argument `name = value` (or bare `value`), return `value`
/// (the `name =` prefix stripped). See [`split_named_arg`].
pub(crate) fn strip_named_arg(arg: &str) -> String {
    match split_named_arg(arg) {
        Some((_, value)) => value,
        None => arg.trim().to_string(),
    }
}

/// True iff `expr` contains a `<recv>.<ident>(` *method-call* shape at an
/// identifier boundary, outside string literals and comments — e.g. a
/// `self.foo(` call (as opposed to a bare field access `self.foo` or the token
/// inside a `"self.foo("` literal). Used to decide whether an argument borrows
/// the receiver and must be hoisted (#150).
pub fn contains_receiver_call(
    expr: &str,
    lang: crate::frame_c::visitors::TargetLanguage,
    receiver: &str,
) -> bool {
    let needle = format!("{receiver}.");
    let nb = needle.as_bytes();
    let skipper = crate::frame_c::compiler::native_region_scanner::create_skipper(lang);
    let bytes = expr.as_bytes();
    let end = bytes.len();
    let mut i = 0usize;
    while i < end {
        if let Some(next) = skipper.skip_string(bytes, i, end) {
            i = next;
            continue;
        }
        if let Some(next) = skipper.skip_comment(bytes, i, end) {
            i = next;
            continue;
        }
        let boundary = i == 0 || !(bytes[i - 1].is_ascii_alphanumeric() || bytes[i - 1] == b'_');
        if boundary && bytes[i..].starts_with(nb) {
            // Walk the identifier after `<recv>.`; a `(` right after it is a call.
            let mut j = i + nb.len();
            let id_start = j;
            while j < end && (bytes[j].is_ascii_alphanumeric() || bytes[j] == b'_') {
                j += 1;
            }
            if j > id_start && j < end && bytes[j] == b'(' {
                return true;
            }
        }
        i += 1;
    }
    false
}

/// Byte width of the UTF-8 character that starts with `first_byte`.
/// Returns 1 for ASCII and any unexpected continuation byte (which
/// should never appear at an iteration boundary since we always
/// advance by full character widths).
pub(crate) fn utf8_char_len(first_byte: u8) -> usize {
    if first_byte < 0x80 {
        1
    } else if first_byte < 0xC0 {
        1 // continuation byte — defensive
    } else if first_byte < 0xE0 {
        2
    } else if first_byte < 0xF0 {
        3
    } else {
        4
    }
}

/// RFC-0056 P9 (#209): the per-system **input adapter** — the borrowed input
/// source, one table, single-sourced.
///
/// The adapter holds the caller's buffer **by reference** (no copy) and exposes
/// two *indexed accessors*, `get(i)` and `len()`. That is the whole abstraction:
/// **not a borrow concept** — an accessor. Which is why it ports at all.
///
/// * GC targets — a field is already a reference; the adapter only supplies the
///   accessor spelling. Zero copy, no ceremony.
/// * C / C++ — `ptr + len`.
/// * Rust — does NOT use this: it needs a real generic + trait so the borrow
///   checker can see the lifetime (emitted in `backends/rust.rs`). Same
///   accessors, different mechanism.
///
/// Duplicating this across seventeen backend arms is exactly the drift generator
/// RFC-0056 P11 warns about — five zero-literal tables and three disagreeing
/// literal emitters already exist. One table.
/// RFC-0056 P9: the target's spelling for the RAW buffer a borrowing system is
/// constructed from — the type of `over(src)`'s argument, before the adapter wraps
/// it. Same table, same reason (P11).
pub(crate) fn input_buffer_type(lang: TargetLanguage) -> &'static str {
    match lang {
        TargetLanguage::Java | TargetLanguage::CSharp => "byte[]",
        TargetLanguage::Kotlin => "ByteArray",
        TargetLanguage::Swift => "[UInt8]",
        TargetLanguage::Dart => "List<int>",
        TargetLanguage::TypeScript => "ArrayLike<number>",
        TargetLanguage::Go => "[]byte",
        TargetLanguage::Cpp | TargetLanguage::C => "const unsigned char*",
        // dynamically typed: no annotation
        _ => "",
    }
}

pub(crate) fn input_adapter(lang: TargetLanguage, sys: &str, elem: &str) -> String {
    let _ = elem;
    match lang {
        TargetLanguage::Python3 => format!(
            "class {sys}Input:\n    \
             \"\"\"Borrowed input source (RFC-0056 P9). Holds the caller's buffer by reference.\"\"\"\n    \
             __slots__ = ('_b',)\n    \
             def __init__(self, b): self._b = b\n    \
             def get(self, i): return self._b[i]\n    \
             def len(self): return len(self._b)\n\n"
        ),
        TargetLanguage::TypeScript => format!(
            "class {sys}Input {{\n    \
             constructor(private readonly b: ArrayLike<number>) {{}}\n    \
             get(i: number): number {{ return this.b[i]; }}\n    \
             len(): number {{ return this.b.length; }}\n}}\n\n"
        ),
        TargetLanguage::JavaScript => format!(
            "class {sys}Input {{\n    \
             constructor(b) {{ this.b = b; }}\n    \
             get(i) {{ return this.b[i]; }}\n    \
             len() {{ return this.b.length; }}\n}}\n\n"
        ),
        TargetLanguage::Java => format!(
            "final class {sys}Input {{\n    \
             private final byte[] b;\n    \
             {sys}Input(byte[] b) {{ this.b = b; }}\n    \
             int get(int i) {{ return b[i] & 0xFF; }}\n    \
             int len() {{ return b.length; }}\n}}\n\n"
        ),
        TargetLanguage::CSharp => format!(
            "sealed class {sys}Input {{\n    \
             private readonly byte[] b;\n    \
             public {sys}Input(byte[] b) {{ this.b = b; }}\n    \
             public int get(int i) => b[i];\n    \
             public int len() => b.Length;\n}}\n\n"
        ),
        TargetLanguage::Kotlin => format!(
            "class {sys}Input(private val b: ByteArray) {{\n    \
             fun get(i: Int): Int = b[i].toInt() and 0xFF\n    \
             fun len(): Int = b.size\n}}\n\n"
        ),
        TargetLanguage::Swift => format!(
            "final class {sys}Input {{\n    \
             private let b: [UInt8]\n    \
             init(_ b: [UInt8]) {{ self.b = b }}\n    \
             func get(_ i: Int) -> UInt8 {{ return b[i] }}\n    \
             func len() -> Int {{ return b.count }}\n}}\n\n"
        ),
        TargetLanguage::Dart => format!(
            "class {sys}Input {{\n    \
             final List<int> _b;\n    \
             {sys}Input(this._b);\n    \
             int get(int i) => _b[i];\n    \
             int len() => _b.length;\n}}\n\n"
        ),
        TargetLanguage::Go => format!(
            "type {sys}Input struct {{ b []byte }}\n\
             func (in {sys}Input) Get(i int) byte {{ return in.b[i] }}\n\
             func (in {sys}Input) Len() int {{ return len(in.b) }}\n\n"
        ),
        TargetLanguage::Ruby => format!(
            "class {sys}Input\n  \
             def initialize(b); @b = b; end\n  \
             def get(i); @b[i]; end\n  \
             def len; @b.length; end\nend\n\n"
        ),
        TargetLanguage::Php => format!(
            "final class {sys}Input {{\n    \
             private $b;\n    \
             public function __construct($b) {{ $this->b = $b; }}\n    \
             public function get($i) {{ return $this->b[$i]; }}\n    \
             public function len() {{ return count($this->b); }}\n}}\n\n"
        ),
        TargetLanguage::Lua => format!(
            "local {sys}Input = {{}}\n\
             {sys}Input.__index = {sys}Input\n\
             function {sys}Input.new(b) return setmetatable({{ b = b }}, {sys}Input) end\n\
             function {sys}Input:get(i) return self.b[i] end\n\
             function {sys}Input:len() return #self.b end\n\n"
        ),
        TargetLanguage::GDScript => format!(
            "class {sys}Input:\n    \
             var _b\n    \
             func _init(b): _b = b\n    \
             func get(i): return _b[i]\n    \
             func len(): return _b.size()\n\n"
        ),
        TargetLanguage::Cpp => format!(
            "struct {sys}Input {{\n    \
             const unsigned char* b; std::size_t n;\n    \
             unsigned char get(std::size_t i) const {{ return b[i]; }}\n    \
             std::size_t len() const {{ return n; }}\n}};\n\n"
        ),
        TargetLanguage::C => format!(
            "typedef struct {{ const unsigned char* b; size_t n; }} {sys}Input;\n\
             static unsigned char {sys}Input_get({sys}Input in, size_t i) {{ return in.b[i]; }}\n\
             static size_t {sys}Input_len({sys}Input in) {{ return in.n; }}\n\n"
        ),
        // Rust emits a real generic + trait in its own backend (the borrow checker
        // needs to see the lifetime). GraphViz has no runtime.
        TargetLanguage::Rust | TargetLanguage::Graphviz => String::new(),
    }
}

#[cfg(test)]
mod tests {

    #[test]
    fn matching_close_paren_string_and_depth_aware() {
        use crate::frame_c::visitors::TargetLanguage as L;
        // plain nesting
        assert_eq!(matching_close_paren("(a + (b))", L::Kotlin, 1), Some(8));
        // a `)` inside a string does not close
        let s = r#"(f(")") + 1)"#;
        assert_eq!(matching_close_paren(s, L::Kotlin, 1), Some(s.len() - 1));
        // unbalanced → None
        assert_eq!(matching_close_paren("(a + (b)", L::Kotlin, 1), None);
    }

    use super::*;
    use crate::frame_c::compiler::frame_ast::{Expression, Literal, Type};
    use crate::frame_c::visitors::TargetLanguage;

    // =========================================================
    // =========================================================
    // state_var_initializer — verbatim passthrough, no synthesis
    // =========================================================

    #[test]
    fn test_state_var_initializer_is_verbatim() {
        // The user's initializer text is emitted untouched — Frame does
        // not interpret values (issue #84; E610 guarantees presence).
        let var = crate::frame_c::compiler::frame_ast::StateVarAst {
            name: "cool".to_string(),
            var_type: Type::Custom("float".to_string()),
            initializer_text: Some("0.25f /* native */".to_string()),
            span: crate::frame_c::compiler::frame_ast::Span::new(0, 0),
        };
        assert_eq!(state_var_initializer(&var), "0.25f /* native */");
    }

    #[test]
    #[should_panic(expected = "E610")]
    fn test_state_var_initializer_missing_is_unreachable() {
        // E610 rejects this before codegen; reaching the helper without
        // an initializer is an internal invariant violation.
        let var = crate::frame_c::compiler::frame_ast::StateVarAst {
            name: "cool".to_string(),
            var_type: Type::Custom("int".to_string()),
            initializer_text: None,
            span: crate::frame_c::compiler::frame_ast::Span::new(0, 0),
        };
        let _ = state_var_initializer(&var);
    }

    // =========================================================
    // cpp_wrap_any_arg — C++ std::any wrapping for string literals
    // =========================================================

    #[test]
    fn test_cpp_wrap_any_arg_string_literal() {
        assert_eq!(cpp_wrap_any_arg("\"hello\""), "std::string(\"hello\")");
    }

    #[test]
    fn test_cpp_wrap_any_arg_integer() {
        assert_eq!(cpp_wrap_any_arg("42"), "42");
    }

    #[test]
    fn test_cpp_wrap_any_arg_variable() {
        assert_eq!(cpp_wrap_any_arg("my_var"), "my_var");
    }

    #[test]
    fn test_cpp_wrap_any_arg_empty_string() {
        assert_eq!(cpp_wrap_any_arg("\"\""), "std::string(\"\")");
    }

    #[test]
    fn test_cpp_wrap_any_arg_with_whitespace() {
        assert_eq!(cpp_wrap_any_arg("  \"hello\"  "), "std::string(\"hello\")");
    }

    // =========================================================
    // replace_outside_strings_and_comments
    // =========================================================

    #[test]
    fn replace_outside_strings_basic_match() {
        // No strings or comments — straightforward replace.
        let out = replace_outside_strings_and_comments(
            "let x = self.y",
            TargetLanguage::Rust,
            &[("self.", "s.")],
        );
        assert_eq!(out, "let x = s.y");
    }

    #[test]
    fn replace_outside_strings_spares_string_literals() {
        // `self.` inside a double-quoted string must survive.
        let out = replace_outside_strings_and_comments(
            r#"let msg = "self.x is untouched"; self.y = 1;"#,
            TargetLanguage::Rust,
            &[("self.", "s.")],
        );
        assert_eq!(out, r#"let msg = "self.x is untouched"; s.y = 1;"#);
    }

    #[test]
    fn replace_outside_strings_spares_line_comments_rust() {
        let out = replace_outside_strings_and_comments(
            "// self.should stay\nself.y = 1;",
            TargetLanguage::Rust,
            &[("self.", "s.")],
        );
        assert_eq!(out, "// self.should stay\ns.y = 1;");
    }

    #[test]
    fn replace_outside_strings_handles_escapes() {
        // `\"` inside a string shouldn't terminate it early.
        let out = replace_outside_strings_and_comments(
            r#"let s = "outer \"self.inner\" still in string"; self.done = 1;"#,
            TargetLanguage::Rust,
            &[("self.", "s.")],
        );
        assert_eq!(
            out,
            r#"let s = "outer \"self.inner\" still in string"; s.done = 1;"#
        );
    }

    #[test]
    fn replace_outside_strings_multiple_rules() {
        // Multiple rules — first match at position wins.
        let out = replace_outside_strings_and_comments(
            "True False true",
            TargetLanguage::Rust,
            &[("True", "true"), ("False", "false")],
        );
        assert_eq!(out, "true false true");
    }

    #[test]
    fn replace_outside_strings_utf8_passthrough() {
        // Non-ASCII identifiers advance by full UTF-8 width.
        let out = replace_outside_strings_and_comments(
            "let café = self.x",
            TargetLanguage::Rust,
            &[("self.", "s.")],
        );
        assert_eq!(out, "let café = s.x");
    }

    #[test]
    fn replace_outside_strings_works_for_go() {
        // Go line comments use `//`, same as Rust.
        let out = replace_outside_strings_and_comments(
            "self.x = 1 // self.inside_comment\nself.y = 2",
            TargetLanguage::Go,
            &[("self.", "s.")],
        );
        assert_eq!(out, "s.x = 1 // self.inside_comment\ns.y = 2");
    }

    #[test]
    fn word_start_replace_is_boundary_and_string_safe() {
        use crate::frame_c::visitors::TargetLanguage;
        // receiver at a left word boundary -> rewritten
        assert_eq!(
            replace_word_start_outside_strings_and_comments(
                "s.__compartment = s.__prepareEnter(a)",
                TargetLanguage::Go,
                &[("s.", "c.")]
            ),
            "c.__compartment = c.__prepareEnter(a)"
        );
        // tail of a longer identifier -> NOT rewritten
        assert_eq!(
            replace_word_start_outside_strings_and_comments(
                "x := sensors.value + s.n",
                TargetLanguage::Go,
                &[("s.", "c.")]
            ),
            "x := sensors.value + c.n"
        );
        // inside a string literal -> NOT rewritten
        assert_eq!(
            replace_word_start_outside_strings_and_comments(
                "s.msg = \"logs. done\"",
                TargetLanguage::Go,
                &[("s.", "c.")]
            ),
            "c.msg = \"logs. done\""
        );
        // inside a comment -> NOT rewritten
        assert_eq!(
            replace_word_start_outside_strings_and_comments(
                "c.x = 1 // reset s.state",
                TargetLanguage::Go,
                &[("s.", "c.")]
            ),
            "c.x = 1 // reset s.state"
        );
    }

    #[test]
    fn split_and_receiver_call_helpers() {
        use crate::frame_c::visitors::TargetLanguage::Rust;
        // depth-aware split: commas inside nested calls / brackets are not splits
        assert_eq!(
            split_top_level_args("self.a(x), self.b(y, z)", Rust),
            vec!["self.a(x)".to_string(), "self.b(y, z)".to_string()]
        );
        assert_eq!(
            split_top_level_args("f(a, b)", Rust),
            vec!["f(a, b)".to_string()]
        );
        assert_eq!(split_top_level_args("", Rust), Vec::<String>::new());
        // comma inside a string literal is not a split point
        assert_eq!(
            split_top_level_args("\"a,b\", c", Rust),
            vec!["\"a,b\"".to_string(), "c".to_string()]
        );
        // receiver-call detection: call vs field vs string vs boundary
        assert!(contains_receiver_call("self.bar(x)", Rust, "self"));
        assert!(!contains_receiver_call("self.field", Rust, "self")); // field, no call
        assert!(!contains_receiver_call("\"self.bar(x)\"", Rust, "self")); // inside string
        assert!(!contains_receiver_call("myself.bar(x)", Rust, "self")); // boundary
    }

    #[test]
    fn arg_values_and_named_split() {
        use crate::frame_c::visitors::TargetLanguage::Rust;
        // nested-comma value kept whole; named-arg value stripped
        assert_eq!(
            arg_values("clamp(1, 2), 9", Rust),
            vec!["clamp(1, 2)".to_string(), "9".to_string()]
        );
        assert_eq!(
            arg_values("pt = point(1, 2), hi = 9", Rust),
            vec!["point(1, 2)".to_string(), "9".to_string()]
        );
        // a `==` comparison value is NOT treated as a named-arg separator
        assert_eq!(arg_values("a == b", Rust), vec!["a == b".to_string()]);
        // split_named_arg pairs
        assert_eq!(
            split_named_arg("x = f(1, 2)"),
            Some(("x".to_string(), "f(1, 2)".to_string()))
        );
        assert_eq!(split_named_arg("f(a=b)"), None); // nested `=`, not named
        assert_eq!(split_named_arg("plain"), None);
    }

    #[test]
    fn init_runtime_expression_classifier() {
        use crate::frame_c::visitors::TargetLanguage::Php;
        // constants -> not runtime
        for c in [
            "5",
            "-3",
            "(1 + 2)",
            "[1, 2, 3]",
            "MAX_SIZE",
            "Foo::BAR",
            "\"foo(bar)\"",
            "'has new inside'",
        ] {
            assert!(
                !init_is_runtime_expression(c, Php),
                "`{c}` should be constant"
            );
        }
        // runtime -> needs constructor
        for r in [
            "new X()",
            "@@Sensor()",
            "make_thing()",
            "Vec2(640, 480)",
            "\"a\" . new Y() . \"b\"",
            "foo() + 1",
        ] {
            assert!(
                init_is_runtime_expression(r, Php),
                "`{r}` should be runtime"
            );
        }
    }
}
