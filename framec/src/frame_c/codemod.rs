//! Codemods for migrating Frame source between RFC-defined contract changes.
//!
//! RFC-0043 Phase 1: `add_async_attr` inserts `@@[async]` on any
//! `@@system` whose body declares async members but whose header lacks
//! the attribute. Idempotent. Operates on Frame source text without
//! invoking the full parser.
//!
//! Exposed via two surfaces:
//!   - **CLI:** `framec project add-async-attr <path>` walks a tree
//!     and rewrites files in place.
//!   - **Library / WASM:** [`add_async_attr_to_source`] is a pure
//!     `&str -> String` function suitable for the framec-wasm
//!     `migrate_async_attr` export and for in-process tooling.

use regex::Regex;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

/// Per-tree migration result for the CLI walker.
#[derive(Debug, Default)]
pub struct MigrationReport {
    pub files_scanned: usize,
    pub files_modified: usize,
    pub modified_paths: Vec<PathBuf>,
}

/// Default Frame source extensions handled by the codemod tree walker.
/// Mirrors the file-extension table in the project README.
pub const FRAME_SOURCE_EXTENSIONS: &[&str] = &[
    "fpy", "frs", "fts", "fjs", "fjava", "fkt", "fswift", "fdart", "fgd", "flua", "fphp", "frb",
    "fc", "fcpp", "fcs", "fgo", "ferl", "frm",
];

/// Insert `@@[async]` before any `@@system` whose body declares async
/// members and whose header does not already carry the attribute.
///
/// Idempotent: running on already-migrated source returns it unchanged.
///
/// # Limitations
///
/// - Brace counting is naive — does not skip braces inside string
///   literals or comments. For typical Frame source (where strings'
///   braces are balanced, e.g. f-strings), this works correctly.
/// - The "async member" check is a textual scan for the pattern
///   `async <ident>(` inside the system body. False positives are
///   possible if comments inside a sync system mention this pattern.
///   Re-running the codemod on the result is safe (idempotent), so
///   recovery from a false positive is to remove the attribute by hand.
pub fn add_async_attr_to_source(src: &str) -> String {
    let re_system = system_with_attrs_regex();
    let re_async_member = async_member_regex();

    let mut result = String::with_capacity(src.len() + 32);
    let mut cursor = 0;

    for caps in re_system.captures_iter(src) {
        let full_match = caps.get(0).unwrap();
        let attrs = caps.get(1).map(|m| m.as_str()).unwrap_or("");
        let header_match = caps.get(2).unwrap();

        result.push_str(&src[cursor..full_match.start()]);

        let header_end = header_match.end();
        let body_open = match src[header_end..].find('{') {
            Some(p) => header_end + p,
            None => {
                result.push_str(full_match.as_str());
                cursor = full_match.end();
                continue;
            }
        };
        let body_close = match find_matching_brace(src, body_open) {
            Some(p) => p,
            None => {
                result.push_str(full_match.as_str());
                cursor = full_match.end();
                continue;
            }
        };

        let body = &src[body_open + 1..body_close];
        let has_async_member = re_async_member.is_match(body);
        let already_has_attr = attribute_block_contains_async(attrs);

        if has_async_member && !already_has_attr {
            result.push_str(attrs);
            result.push_str("@@[async]\n");
            result.push_str(header_match.as_str());
        } else {
            result.push_str(full_match.as_str());
        }
        cursor = full_match.end();
    }

    result.push_str(&src[cursor..]);
    result
}

/// Walk a directory tree, find Frame source files (by extension), and
/// rewrite each in place. Returns a report of files scanned and modified.
pub fn add_async_attr_to_tree(root: &Path, extensions: &[&str]) -> io::Result<MigrationReport> {
    let mut report = MigrationReport::default();
    walk_dir(root, extensions, &mut report)?;
    Ok(report)
}

// ---------------------------------------------------------------------------
// Internals
// ---------------------------------------------------------------------------

/// Captures the contiguous block of `@@[...]` attributes immediately
/// preceding a `@@system <Name>` header. Group 1 is the attribute block
/// (possibly empty); group 2 is the system header.
fn system_with_attrs_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"((?:@@\[[^\]]+\][ \t]*\n[ \t]*)*)(@@system[ \t]+\w+\s*)").unwrap()
    })
}

/// Matches a Frame async method declaration: the `async` keyword
/// followed by an identifier and an opening paren.
fn async_member_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"\basync\s+\w+\s*\(").unwrap())
}

/// Does the attribute block (sequence of `@@[...]` lines) already
/// contain `@@[async]` exactly? Matches both bare `@@[async]` and
/// variants with surrounding whitespace; rejects e.g. `@@[async_other]`.
fn attribute_block_contains_async(attrs: &str) -> bool {
    static RE: OnceLock<Regex> = OnceLock::new();
    let re = RE.get_or_init(|| Regex::new(r"@@\[\s*async\s*\]").unwrap());
    re.is_match(attrs)
}

/// Given an index of an opening `{` in `src`, returns the index of the
/// matching `}` using naive brace counting (does not skip braces inside
/// string literals or comments).
fn find_matching_brace(src: &str, open_pos: usize) -> Option<usize> {
    let bytes = src.as_bytes();
    let mut depth = 0_i32;
    for i in open_pos..bytes.len() {
        match bytes[i] {
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(i);
                }
            }
            _ => {}
        }
    }
    None
}

fn walk_dir(dir: &Path, extensions: &[&str], report: &mut MigrationReport) -> io::Result<()> {
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            walk_dir(&path, extensions, report)?;
        } else if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
            if extensions.contains(&ext) {
                report.files_scanned += 1;
                let src = fs::read_to_string(&path)?;
                let migrated = add_async_attr_to_source(&src);
                if migrated != src {
                    fs::write(&path, &migrated)?;
                    report.files_modified += 1;
                    report.modified_paths.push(path);
                }
            }
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inserts_attr_for_async_system_without_attr() {
        let src = "@@system Foo {\n    interface:\n        async fetch(): str\n}\n";
        let out = add_async_attr_to_source(src);
        assert!(
            out.contains("@@[async]\n@@system Foo"),
            "expected @@[async] before @@system Foo; got:\n{out}"
        );
    }

    #[test]
    fn idempotent_when_attr_already_present() {
        let src = "@@[async]\n@@system Foo {\n    interface:\n        async fetch(): str\n}\n";
        let out = add_async_attr_to_source(src);
        assert_eq!(out, src, "expected no change on already-migrated source");
    }

    #[test]
    fn no_change_for_sync_system() {
        let src = "@@system Foo {\n    interface:\n        bump()\n}\n";
        let out = add_async_attr_to_source(src);
        assert_eq!(out, src);
    }

    #[test]
    fn no_change_when_attr_present_without_async_members() {
        // @@[async] permitted on a sync-dispatch system per RFC-0043
        let src = "@@[async]\n@@system Foo {\n    interface:\n        bump()\n}\n";
        let out = add_async_attr_to_source(src);
        assert_eq!(out, src);
    }

    #[test]
    fn handles_multiple_systems_per_file() {
        let src = "@@system Sync {\n    interface:\n        bump()\n}\n\n\
                   @@system Async {\n    interface:\n        async fetch(): str\n}\n";
        let out = add_async_attr_to_source(src);
        assert!(
            out.contains("@@system Sync"),
            "expected Sync system unchanged"
        );
        assert!(
            !out.contains("@@[async]\n@@system Sync"),
            "must not insert before sync system"
        );
        assert!(
            out.contains("@@[async]\n@@system Async"),
            "expected @@[async] inserted before async system"
        );
    }

    #[test]
    fn preserves_other_attributes() {
        let src = "@@[persist(str)]\n@@[save(snap)]\n@@[load(restore)]\n\
                   @@system Counter {\n    interface:\n        async bump(): int\n}\n";
        let out = add_async_attr_to_source(src);
        assert!(out.contains("@@[persist(str)]"));
        assert!(out.contains("@@[save(snap)]"));
        assert!(out.contains("@@[load(restore)]"));
        assert!(out.contains("@@[async]"));
        // @@[async] must sit immediately before @@system, after the other attributes
        let async_pos = out.find("@@[async]").unwrap();
        let system_pos = out.find("@@system").unwrap();
        let load_pos = out.find("@@[load").unwrap();
        assert!(load_pos < async_pos);
        assert!(async_pos < system_pos);
    }

    #[test]
    fn ignores_async_in_native_code_outside_system() {
        // Native Python `async def` outside a system body shouldn't
        // trigger insertion on a sync system that follows.
        let src = "import asyncio\n\n\
                   async def helper():\n    pass\n\n\
                   @@system Sync {\n    interface:\n        bump()\n}\n";
        let out = add_async_attr_to_source(src);
        assert_eq!(out, src);
    }

    #[test]
    fn picks_up_async_action() {
        let src = "@@system Foo {\n    interface:\n        bump()\n    actions:\n        async io_work(): str\n}\n";
        let out = add_async_attr_to_source(src);
        assert!(out.contains("@@[async]\n@@system Foo"));
    }

    #[test]
    fn picks_up_async_operation() {
        let src = "@@system Foo {\n    interface:\n        bump()\n    operations:\n        async get_state(): str\n}\n";
        let out = add_async_attr_to_source(src);
        assert!(out.contains("@@[async]\n@@system Foo"));
    }

    #[test]
    fn idempotent_on_double_run() {
        let src = "@@system Foo {\n    interface:\n        async fetch(): str\n}\n";
        let pass1 = add_async_attr_to_source(src);
        let pass2 = add_async_attr_to_source(&pass1);
        assert_eq!(pass1, pass2, "second pass should be a no-op");
    }

    #[test]
    fn preserves_leading_native_code() {
        let src = "@@[target(\"python_3\")]\n\nimport asyncio\n\n\
                   @@system Foo {\n    interface:\n        async fetch(): str\n}\n";
        let out = add_async_attr_to_source(src);
        assert!(out.starts_with("@@[target(\"python_3\")]\n\nimport asyncio\n"));
        assert!(out.contains("@@[async]\n@@system Foo"));
    }

    #[test]
    fn finds_matching_brace_basic() {
        let src = "abc { def } ghi";
        let open = src.find('{').unwrap();
        let close = find_matching_brace(src, open).unwrap();
        assert_eq!(&src[open..=close], "{ def }");
    }

    #[test]
    fn finds_matching_brace_nested() {
        let src = "a { b { c } d } e";
        let open = src.find('{').unwrap();
        let close = find_matching_brace(src, open).unwrap();
        assert_eq!(&src[open..=close], "{ b { c } d }");
    }
}
