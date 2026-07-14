//! **Composition works, proven by running.** A scan system that skips each `"`-string by
//! invoking the StringScan SYSTEM as a native leaf, over the same borrowed `&[u8]`.

use frame_compiler::text::scan::string_counter::count;

#[test]
fn strings_are_counted_by_composing_stringscan() {
    // Three strings; the braces/quotes INSIDE them must not be miscounted, because the walk
    // skips each string's interior via the composed StringScan.
    assert_eq!(count(br#"a = "one"; b = "t{w}o"; c = "th\"ree""#), 3);
    // No strings.
    assert_eq!(count(b"x = 1 + 2;"), 0);
    // A `"` inside a string does not start a new one (escaped), and the closing quote of one
    // string is not the opening of the next.
    assert_eq!(count(br#""a""b""c""#), 3);
    // Empty input.
    assert_eq!(count(b""), 0);
}
