//! `Atom` and `Place` — **the types that make the precedence bug unrepresentable.**
//!
//! # The bug
//!
//! framec splices Frame references into native expressions it does not parse and must
//! never parse. That is sound **only if the spliced text behaves as a single operand
//! wherever it lands.**
//!
//! The old compiler expanded `$.x` in C# to a **bare cast**:
//!
//! ```csharp
//! (int) compartment.state_vars["n"].Doubled()
//! ```
//!
//! A C# cast is *unary* precedence; `.` is *primary*. So that parses as
//! `(int)( x.Doubled() )` — the cast lands on the wrong side, `Doubled()` binds on
//! `object`, and with an overload present the program **compiles clean, exits 0, and
//! prints the wrong answer** (`-1` instead of `84`). #213.
//!
//! The same shape shipped four more times: C's cast, C's `*` deref for boxed structs,
//! Rust's block expression, and `await` at the head on eight targets (`await x.f()`
//! invokes `f` on the **Promise**).
//!
//! # The law
//!
//! > **Every Frame-reference expansion MUST be an ATOM in the target grammar** — an
//! > identifier, a literal, a parenthesized expression, or an unbroken postfix chain
//! > (`a.b`, `a[i]`, `f(x)`, `x.(T)`) rooted at one of those.
//!
//! Falsifiable: for expansion `E`, all of `f(E)`, `-E`, `E.m()`, `E[i]`, `a*E+b` must
//! parse with `E` as a single operand.
//!
//! # Why it is a type and not a rule
//!
//! Because **review has already failed at this three times, in the same place, on the
//! same rule** — and the correct code was sitting sixty-five lines away in a sibling
//! file the whole time.
//!
//! So: [`Atom`] has **no `raw(String)` constructor.** The only way to get a cast, a
//! deref, or an `await` into the output is through a constructor that **parenthesizes
//! it**. A backend cannot emit a non-atom, because there is no function that would
//! return one.
//!
//! # And why `Place` is a separate type
//!
//! Because the atom rule is a rule about **reads**. `((int) m["x"]) = 1` is a compile
//! error in C# and Java. And `@@:self.field` is the one reference that is *both* a read
//! (`x = @@:self.field`) and an lvalue root (`@@:self.field = 3`) — so "wrap everything"
//! would break it.
//!
//! [`Place`] therefore has **no `group()` and no `cast()`**. It can only ever be an
//! unparenthesized designator. And every `Place` *is* a valid `Atom`
//! ([`Place::into_atom`]) — but not the reverse, which is exactly the asymmetry the old
//! compiler could not express, because it used a `String` for both.

use std::fmt;

/// Target-language expression text, **guaranteed to be an atom**.
///
/// Safe to splice into any native expression framec has not parsed — which is every
/// native expression, by design.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Atom(String);

impl Atom {
    /// An identifier or a literal. `n`, `self`, `42`, `"hi"`. Atomic.
    pub fn ident(s: impl Into<String>) -> Atom {
        Atom(s.into())
    }

    /// A call: `f(a, b)`. **Primary** in every target grammar — atomic.
    ///
    /// Note the args are handed over as ONE opaque string. framec does **not** split
    /// them and does not count them: in C++, `f(a < b, c > d)` (two comparisons) and
    /// `f(std::map<int,int>())` (one generic) are the same token shape, and telling them
    /// apart needs name lookup over the user's types that C++'s own grammar cannot do.
    /// The target compiler splits it, and hands the arity error back for free.
    pub fn call(callee: impl fmt::Display, args: impl fmt::Display) -> Atom {
        Atom(format!("{callee}({args})"))
    }

    /// `base.field` — postfix. Still atomic, because `base` already is.
    pub fn field(base: Atom, f: impl fmt::Display) -> Atom {
        Atom(format!("{}.{}", base.0, f))
    }

    /// `base[key]` — postfix. Still atomic.
    pub fn index(base: Atom, key: impl fmt::Display) -> Atom {
        Atom(format!("{}[{}]", base.0, key))
    }

    /// `base.method(args)` — postfix. Still atomic.
    pub fn method(base: Atom, m: impl fmt::Display, args: impl fmt::Display) -> Atom {
        Atom(format!("{}.{}({})", base.0, m, args))
    }

    /// **A C-style cast — ALWAYS PARENTHESIZED.** `((int) x)`, never `(int) x`.
    ///
    /// This one function is #213. There is no way to spell the bare form, because there
    /// is no constructor that would produce it.
    pub fn cast(ty: impl fmt::Display, inner: Atom) -> Atom {
        Atom(format!("(({ty}) {})", inner.0))
    }

    /// `(x as T)` / `(x as! T)` — Kotlin, Swift, Dart. Parenthesized.
    pub fn as_cast(inner: Atom, op: &str, ty: impl fmt::Display) -> Atom {
        Atom(format!("({} {op} {ty})", inner.0))
    }

    /// `x.(T)` — Go's type assertion. **Postfix**, so it is already an atom.
    pub fn type_assert(inner: Atom, ty: impl fmt::Display) -> Atom {
        Atom(format!("{}.({})", inner.0, ty))
    }

    /// **`(await x)` — PARENTHESIZED.** `await x.f()` invokes `f` on the *Promise*.
    ///
    /// This is #225, on eight targets. Rust is the only one that got it right, because
    /// its `.await` is postfix. Here nobody can get it wrong.
    pub fn awaited(inner: Atom, kw: &str) -> Atom {
        Atom(format!("({kw} {})", inner.0))
    }

    /// **`(*x)` — PARENTHESIZED.** C's boxed-struct deref (#220). A `*` binds looser
    /// than `.`/`->`/`[]`, so `*(T*)x.f` dereferences the wrong thing entirely.
    pub fn deref(inner: Atom) -> Atom {
        Atom(format!("(*{})", inner.0))
    }

    /// An escape hatch for a target construct we have not modelled — **and it
    /// parenthesizes**, because the whole point is that we do not know what is inside.
    ///
    /// If you are reaching for this to emit something that is *already* an atom, you
    /// want `ident`. If you are reaching for it to emit something that is *not*, then
    /// the parens are exactly what you needed and did not think of.
    pub fn group(expr: impl fmt::Display) -> Atom {
        Atom(format!("({expr})"))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

// NOTE what is deliberately absent:
//
//   Atom::raw(String)   — there is no way to put arbitrary text in an Atom.
//   From<String>        — same.
//   Deref<Target=str>   — an Atom is not a string; you cannot grep it.
//
// A backend cannot emit a non-atom because no function returns one. That is the whole
// design, and it is the difference between this and a doc-comment saying "remember to
// parenthesize."

impl fmt::Display for Atom {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Target-language text usable as an **assignment target**.
///
/// A `Place` is an unparenthesized *designator*: an identifier, or a postfix chain
/// rooted at one. It is **not** derivable from an `Atom` by removing parens, and **not
/// every reference has one**.
///
/// `$.x`, `@@:data.k` and `@@:return` write through *container operations*
/// (`map.put(...)`), so they have **no lvalue form at all** — and that is exactly why
/// `$.x += 1` silently emitted `((int) m["x"]) += 1` in the old compiler (#227). It let
/// them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Place(String);

impl Place {
    pub fn ident(s: impl Into<String>) -> Place {
        Place(s.into())
    }

    /// `base.field` — a designator.
    pub fn field(base: Place, f: impl fmt::Display) -> Place {
        Place(format!("{}.{}", base.0, f))
    }

    /// `base[key]` — a designator.
    pub fn index(base: Place, key: impl fmt::Display) -> Place {
        Place(format!("{}[{}]", base.0, key))
    }

    /// Every `Place` is a valid `Atom`. **The reverse is not true**, and there is no
    /// function here that pretends otherwise.
    pub fn into_atom(self) -> Atom {
        Atom(self.0)
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

// NOTE what is deliberately absent from `Place`:
//
//   Place::group  — `(x) = 1` is a compile error in C# and Java.
//   Place::cast   — `((int) m["x"]) = 1` likewise. This is #227.
//   From<Atom>    — an Atom may be a parenthesized cast; a Place may never be.

impl fmt::Display for Place {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The exact expansion that returned the wrong answer in C#.
    #[test]
    fn a_cast_is_always_parenthesized() {
        let read = Atom::cast(
            "int",
            Atom::index(
                Atom::field(Atom::ident("compartment"), "stateVars"),
                "\"n\"",
            ),
        );
        assert_eq!(read.as_str(), "((int) compartment.stateVars[\"n\"])");

        // And it stays an atom under the operations that broke it.
        let called = Atom::method(read.clone(), "doubled", "");
        assert_eq!(
            called.as_str(),
            "((int) compartment.stateVars[\"n\"]).doubled()",
            "the method must bind to the CAST RESULT, not to the raw object (#213)"
        );
    }

    /// `await x.f()` invokes `f` on the Promise. #225, on eight targets.
    #[test]
    fn await_is_always_parenthesized() {
        let a = Atom::awaited(Atom::method(Atom::ident("this"), "val", ""), "await");
        assert_eq!(a.as_str(), "(await this.val())");
        assert_eq!(
            Atom::method(a, "toString", "").as_str(),
            "(await this.val()).toString()",
            "must bind to the AWAITED VALUE, not to the Promise"
        );
    }
}
