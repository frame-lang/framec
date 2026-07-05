//! C-target marshalling categories — the single source of truth for how a
//! declared type travels through the C runtime's `void*` slots (interface
//! params, the `_return` slot).
//!
//! Issue #72 root cause: three sites categorized types independently and
//! disagreed — the return WRITE packed `float|double` via `pack_double`,
//! the interface-wrapper READ unpacked only `double` (so a `float` return
//! emitted an uncompilable `(float)void*` cast), and unknown types (structs)
//! fell into `(void*)(intptr_t)` casts that don't compile at all. Every
//! site now derives its pack/unpack form from `CMarshal`, so the write and
//! read sides cannot drift apart again.
//!
//! ## The ABI per category
//!
//! | Category | write (pack)                          | read (unpack)              |
//! |----------|---------------------------------------|----------------------------|
//! | `Dbl`    | `Sys_pack_double(v)` (**heap box**)   | `Sys_unpack_double(p)`     |
//! |          |                                       | (NULL-safe deref)          |
//! | `Int`    | `(void*)(intptr_t)(v)`                | `(T)(intptr_t)p`           |
//! | `Str`    | `(void*)(intptr_t)(v)` (ptr fits)     | `(const char*)p`           |
//! | `Ptr`    | `(void*)(intptr_t)(v)` (ptr fits)     | `(T)p`                     |
//! | `Vec`    | direct                                | `(Sys_FrameVec*)p`         |
//! | `Dict`   | direct                                | `(Sys_FrameDict*)p`        |
//! | `Boxed`  | params: **stack box** (`&__box`);     | `*(T*)p` (deref-copy)      |
//! |          | returns: **heap box** (`malloc`+copy) |                            |
//!
//! `Boxed` is the fallback for any type framec doesn't recognize — structs
//! by value (`Vector2`), scalar typedefs, anything. Copy-through-a-box is
//! type-correct for *every* C object type, so framec stays type-ignorant
//! (no "is this a struct?" knowledge — see
//! docs/contributing/type-ignorant-codegen.md).
//!
//! ## Boxed ownership contract
//!
//! - **Params**: the interface wrapper stack-allocates one local per boxed
//!   param and pushes its address. The kernel dispatch is synchronous and
//!   completes before the wrapper returns, so the locals outlive every
//!   reader (including HSM forwards, which reuse the same `_parameters`
//!   vec). Zero allocations; `FrameVec_destroy` never frees pointees.
//! - **Returns**: each boxed `_return` write heap-allocates (freeing any
//!   previous box first — only box-category writes can have written the
//!   slot for a box-category method). The interface wrapper's read
//!   deref-copies and frees: exactly one free, at the end of the call.
//!   A mid-handler `@@:return` read deref-copies WITHOUT freeing (the
//!   context is still live).
//!
//! ## Dbl ownership contract (#81)
//!
//! `Dbl` used to bit-pun the double into the pointer itself, which silently
//! corrupts on 32-bit pointer targets (wasm32). It now travels as a real
//! heap box (`pack_double` mallocs an 8-byte cell; `unpack_double` is a
//! NULL-safe deref), so it follows the same ownership shape as `Boxed`:
//!
//! - **Params**: the interface wrapper stack-boxes (`double __box_p = p;`
//!   push `&__box_p`) — zero allocations, locals outlive the synchronous
//!   dispatch.
//! - **Returns**: each `_return` write frees any previous box, then
//!   heap-boxes; the interface wrapper frees after its final read.
//! - **Containers** (state-vars, state-args): stored via the runtime's
//!   owned-entry APIs (`FrameDict_set_owned` / `FrameVec_push_owned`) —
//!   the container frees on overwrite/destroy and deep-copies on
//!   compartment copy.
//!
//! State-args / enter-args / exit-args keep their historical `intptr_t`
//! fallback for NON-Dbl unknowns: the transition push sites are a separate
//! surface and struct state-args have never been supported on C (tracked
//! as follow-up in #72).

/// How a declared type travels through the C runtime's `void*` slots.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CMarshal {
    /// `float` / `double` — bit-pun via `Sys_pack_double`/`Sys_unpack_double`.
    Dbl,
    /// `int` / `bool` — fits in `intptr_t`.
    Int,
    /// String types — already a pointer; reads cast to `const char*`.
    Str,
    /// Any explicit pointer type (`T*`) — travels as the pointer itself.
    Ptr,
    /// Frame's `list` family — `Sys_FrameVec*`.
    Vec,
    /// Frame's `dict` family — `Sys_FrameDict*`.
    Dict,
    /// Everything else (structs by value, typedefs) — box-by-copy.
    Boxed,
}

/// Categorize a declared type string. The single place this decision is
/// made — every C pack/unpack site routes through here.
pub(crate) fn c_marshal_of(type_str: &str) -> CMarshal {
    let t = type_str.trim();
    match t {
        "float" | "double" | "f32" | "f64" => CMarshal::Dbl,
        "int" | "bool" => CMarshal::Int,
        "str" | "string" | "String" | "char*" | "const char*" => CMarshal::Str,
        "list" | "List" | "Array" | "Array<any>" => CMarshal::Vec,
        "dict" | "Dict" | "Record<string, any>" => CMarshal::Dict,
        _ if t.ends_with('*') => CMarshal::Ptr,
        _ => CMarshal::Boxed,
    }
}

/// The `_return`-slot WRITE for a value expression of the given declared
/// type. `slot` is the lvalue (e.g. `Sys_CTX(self)->_return`). Boxed
/// returns heap-box (freeing any previous box — see module docs).
pub(crate) fn c_return_write(sys: &str, slot: &str, expr: &str, type_str: &str) -> String {
    match c_marshal_of(type_str) {
        // pack_double heap-boxes (#81) — free any previous box first,
        // mirroring the Boxed arm. Only Dbl writes can have written the
        // slot for a Dbl-returning method, so the free is type-safe.
        CMarshal::Dbl => {
            format!("{{ if ({slot}) free({slot}); {slot} = {sys}_pack_double({expr}); }}")
        }
        CMarshal::Boxed => format!(
            "{{ {t}* __rbox = ({t}*)malloc(sizeof(*__rbox)); *__rbox = ({expr}); \
             if ({slot}) free({slot}); {slot} = __rbox; }}",
            t = type_str.trim(),
        ),
        // Int/Str/Ptr/Vec/Dict all travel as (or fit in) the pointer slot, so
        // they share one write. Spelled out explicitly — NOT `_` — so that a
        // future `CMarshal` variant (like `Dbl` in #72, which needed boxing) is
        // a compile error here, exactly as it already is in the exhaustive
        // `c_return_read` mirror below. A silent `_` here would pointer-truncate
        // a new variant on write while `c_return_read` rejected it — the very
        // write/read drift this module exists to make impossible.
        CMarshal::Int | CMarshal::Str | CMarshal::Ptr | CMarshal::Vec | CMarshal::Dict => {
            format!("{slot} = (void*)(intptr_t)({expr});")
        }
    }
}

/// The `_return`-slot READ as an expression of the declared type.
/// `slot` is the rvalue (e.g. `__result_ctx->_return`). Boxed reads
/// deref-copy; freeing is the CALLER's decision (the interface wrapper
/// frees after this read; mid-handler `@@:return` reads must not).
pub(crate) fn c_return_read(sys: &str, slot: &str, type_str: &str) -> String {
    let t = type_str.trim();
    match c_marshal_of(type_str) {
        CMarshal::Dbl => format!("{sys}_unpack_double({slot})"),
        CMarshal::Int => format!("({t})(intptr_t){slot}"),
        // Respect the declared pointer spelling (`char*` stays `char*` —
        // a `(const char*)` read into a `char*` lvalue discards
        // qualifiers); Frame's non-C string spellings get `const char*`.
        CMarshal::Str if t.ends_with('*') => format!("({t}){slot}"),
        CMarshal::Str => format!("(const char*){slot}"),
        CMarshal::Vec => format!("({sys}_FrameVec*){slot}"),
        CMarshal::Dict => format!("({sys}_FrameDict*){slot}"),
        CMarshal::Ptr => format!("({t}){slot}"),
        CMarshal::Boxed => format!("*({t}*){slot}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn float_and_double_are_dbl() {
        // Issue #72: the read side categorized only "double"; float fell
        // through to an uncompilable raw cast. Lock the symmetric set.
        for t in ["float", "double", "f32", "f64"] {
            assert_eq!(c_marshal_of(t), CMarshal::Dbl, "{t}");
        }
    }

    #[test]
    fn structs_and_typedefs_box() {
        for t in ["Vector2", "MyStruct", "size_t", "uint32_t"] {
            assert_eq!(c_marshal_of(t), CMarshal::Boxed, "{t}");
        }
    }

    #[test]
    fn pointers_pass_through() {
        for t in ["Vector2*", "void*", "Demo*"] {
            assert_eq!(c_marshal_of(t), CMarshal::Ptr, "{t}");
        }
    }

    #[test]
    fn write_read_symmetry_float() {
        let w = c_return_write("Demo", "SLOT", "2.5", "float");
        let r = c_return_read("Demo", "SLOT", "float");
        assert!(w.contains("Demo_pack_double(2.5)"), "{w}");
        // #81: pack_double heap-boxes, so the write must free any
        // previous box (repeated @@:return writes in one dispatch).
        assert!(w.contains("if (SLOT) free(SLOT);"), "{w}");
        assert!(r.contains("Demo_unpack_double(SLOT)"), "{r}");
    }

    #[test]
    fn write_read_symmetry_struct() {
        let w = c_return_write("Demo", "SLOT", "v", "Vector2");
        let r = c_return_read("Demo", "SLOT", "Vector2");
        assert!(w.contains("malloc(sizeof(*__rbox))"), "{w}");
        assert!(w.contains("if (SLOT) free(SLOT);"), "{w}");
        assert_eq!(r, "*(Vector2*)SLOT");
    }

    #[test]
    fn intptr_group_writes_are_a_plain_pointer_store() {
        // #123: the Int/Str/Ptr/Vec/Dict group shares one intptr write and is
        // spelled out explicitly (no `_` arm), so a future CMarshal variant is a
        // compile error on both the write and read sides rather than silently
        // pointer-truncating on write. Pin the shared shape per representative.
        for t in ["int", "char*", "Demo*", "list", "dict"] {
            let w = c_return_write("Demo", "SLOT", "x", t);
            assert_eq!(w, "SLOT = (void*)(intptr_t)(x);", "{t}");
            // Not a heap-box write — those belong to Dbl/Boxed only.
            assert!(!w.contains("malloc") && !w.contains("pack_double"), "{t}");
        }
    }
}
