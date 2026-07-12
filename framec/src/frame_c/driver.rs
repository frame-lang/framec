use crate::frame_c::utils::RunError;
pub use crate::frame_c::visitors::TargetLanguage;
use exitcode;
use std::fs;
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::sync::Once;

pub struct Exe {}

impl Exe {
    pub fn new() -> Exe {
        Exe {}
    }

    pub fn run_file(
        &self,
        input_path: &Path,
        target_language: Option<TargetLanguage>,
    ) -> Result<String, RunError> {
        match fs::read_to_string(input_path) {
            Ok(content) => {
                // Compile Frame file. Target precedence: -l/--language >
                // @@[target(...)] pragma > FRAMEC_DEFAULT_TARGET > python_3.
                let lang = resolve_target(target_language, &content);
                let target_lang = crate::frame_c::compiler::TargetLanguage::from(lang);
                let compiler = crate::frame_c::compiler::FrameCompiler::new(target_lang);

                match compiler.compile(&content, input_path.to_str().unwrap_or("<unknown>")) {
                    crate::frame_c::compiler::FrameResult::Ok(output) => Ok(output.code),
                    crate::frame_c::compiler::FrameResult::Err(err) => {
                        let mut error_msg = String::from("Frame compilation errors:\n");
                        for error in err.errors() {
                            error_msg.push_str(&format!("  {}\n", error));
                        }
                        Err(RunError::new(exitcode::DATAERR, &error_msg))
                    }
                }
            }
            Err(err) => Err(RunError::new(
                exitcode::NOINPUT,
                &format!("Cannot read file: {}", err),
            )),
        }
    }

    pub fn run_file_debug(
        &self,
        input_path: &Path,
        target_language: Option<TargetLanguage>,
    ) -> Result<String, RunError> {
        match fs::read_to_string(input_path) {
            Ok(content) => {
                // Compile with debug output. Target precedence: -l/--language >
                // @@[target(...)] pragma > FRAMEC_DEFAULT_TARGET > python_3.
                let lang = resolve_target(target_language, &content);
                let target_lang = crate::frame_c::compiler::TargetLanguage::from(lang);
                let compiler = crate::frame_c::compiler::FrameCompiler::new(target_lang);

                match compiler.compile(&content, input_path.to_str().unwrap_or("<unknown>")) {
                    crate::frame_c::compiler::FrameResult::Ok(output) => Ok(output.code),
                    crate::frame_c::compiler::FrameResult::Err(err) => {
                        let mut error_msg = String::from("Frame compilation errors:\n");
                        for error in err.errors() {
                            error_msg.push_str(&format!("  {}\n", error));
                        }
                        Err(RunError::new(exitcode::DATAERR, &error_msg))
                    }
                }
            }
            Err(err) => Err(RunError::new(
                exitcode::NOINPUT,
                &format!("Cannot read file: {}", err),
            )),
        }
    }

    pub fn run_multifile(
        &self,
        _entry_path: &Path,
        _target_language: Option<TargetLanguage>,
        _output_dir: Option<PathBuf>,
    ) -> Result<String, RunError> {
        Err(RunError::new(
            exitcode::USAGE,
            "Multi-file compilation not yet supported",
        ))
    }

    pub fn run_stdin(&self, target_language: Option<TargetLanguage>) -> Result<String, RunError> {
        let mut buffer = String::new();
        let mut stdin = io::stdin();
        match stdin.read_to_string(&mut buffer) {
            Ok(_size) => {
                // Compile from stdin. Target precedence: -l/--language >
                // @@[target(...)] pragma > FRAMEC_DEFAULT_TARGET > python_3.
                let lang = resolve_target(target_language, &buffer);
                let target_lang = crate::frame_c::compiler::TargetLanguage::from(lang);
                let compiler = crate::frame_c::compiler::FrameCompiler::new(target_lang);

                match compiler.compile(&buffer, "<stdin>") {
                    crate::frame_c::compiler::FrameResult::Ok(output) => Ok(output.code),
                    crate::frame_c::compiler::FrameResult::Err(err) => {
                        let mut error_msg = String::from("Frame compilation errors:\n");
                        for error in err.errors() {
                            error_msg.push_str(&format!("  {}\n", error));
                        }
                        Err(RunError::new(exitcode::DATAERR, &error_msg))
                    }
                }
            }
            Err(err) => Err(RunError::new(
                exitcode::NOINPUT,
                &format!("Cannot read stdin: {}", err),
            )),
        }
    }
}

impl Default for Exe {
    fn default() -> Self {
        Exe::new()
    }
}

/// FRAMEC_BUGS #36: resolve the effective target language with documented,
/// non-breaking precedence:
///
///   `-l/--language` (explicit flag) > `@@[target(...)]` pragma in source >
///   `FRAMEC_DEFAULT_TARGET` env var > built-in `python_3` (with a one-time
///   stderr warning).
///
/// The env var both *sets* the default and counts as an explicit decision,
/// so it suppresses the warning (mirrors the `FRAME_RUNTIME_*_DIR` env idiom).
/// The flag winning over the pragma is the pre-existing behavior — an explicit
/// CLI choice should beat a file's embedded pragma — and is preserved here.
///
/// Emits the warning at most once per process (so `--multifile` doesn't repeat
/// it) and only to stderr (never stdout — must not corrupt
/// `framec foo.frm > out.py`).
pub(crate) fn resolve_target(
    target_language: Option<TargetLanguage>,
    content: &str,
) -> TargetLanguage {
    let env_default = std::env::var("FRAMEC_DEFAULT_TARGET").ok();
    let (lang, diag) = resolve_target_with_diag(target_language, content, env_default);
    if let Some(msg) = diag {
        warn_once(msg);
    }
    lang
}

/// IO-free core of [`resolve_target`]: returns the effective target plus an
/// optional diagnostic message (`None` when the choice was explicit). Kept
/// pure — env is passed in, nothing is printed — so it is unit-testable
/// without stderr capture or env mutation.
fn resolve_target_with_diag(
    target_language: Option<TargetLanguage>,
    content: &str,
    env_default: Option<String>,
) -> (TargetLanguage, Option<String>) {
    // 1. Explicit -l/--language flag wins.
    if let Some(lang) = target_language {
        return (lang, None);
    }
    // 2. @@[target(...)] / @@target pragma in the source.
    if let Some(lang) = detect_at_target(content) {
        return (lang, None);
    }
    // 3. FRAMEC_DEFAULT_TARGET env var — sets the default *and* silences.
    if let Some(v) = env_default
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        return match TargetLanguage::try_from(v) {
            Ok(lang) => (lang, None),
            Err(_) => (
                TargetLanguage::Python3,
                Some(format!(
                    "framec: warning: FRAMEC_DEFAULT_TARGET=\"{v}\" is not a recognized \
                     target; defaulting to python_3. Valid: python_3, typescript, \
                     javascript, rust, c, cpp, java, kotlin, swift, ruby, csharp, go, \
                     php, dart, gdscript, lua, graphviz."
                )),
            ),
        };
    }
    // 4. True fallback — no flag, no pragma, no env default.
    (
        TargetLanguage::Python3,
        Some(
            "framec: warning: no target specified (no @@[target(...)] pragma, no \
             -l/--language); defaulting to python_3. Pass -l, add the pragma, or set \
             FRAMEC_DEFAULT_TARGET=<lang> to silence."
                .to_string(),
        ),
    )
}

/// Emit `msg` to stderr at most once for the lifetime of the process.
fn warn_once(msg: String) {
    static WARNED: Once = Once::new();
    WARNED.call_once(|| eprintln!("{msg}"));
}

/// Detect the `@@[target("...")]` / bare `@@target <lang>` pragma in
/// Frame source files. The bracket form (RFC-0013 wave 2) is the
/// canonical form; the bare form is legacy and hard-cut by E804 at
/// validation time, but the lexer still parses it so we recognize both
/// for backend dispatch.
pub fn detect_at_target(content: &str) -> Option<TargetLanguage> {
    for line in content.lines() {
        let trimmed = line.trim();
        // Skip empty lines and comments
        if trimmed.is_empty() || trimmed.starts_with("//") || trimmed.starts_with('#') {
            continue;
        }
        // Bracket form: @@[target("lang")]  or  @@[target(lang)]
        if let Some(rest) = trimmed.strip_prefix("@@[target") {
            let inner = rest
                .trim_start()
                .strip_prefix('(')?
                .rsplit_once(')')?
                .0
                .trim()
                .trim_matches('"')
                .trim_matches('\'');
            return TargetLanguage::try_from(inner).ok();
        }
        // Bare form: @@target lang (legacy; E804 fails at validation
        // but the detection still helps before that stage)
        if let Some(rest) = trimmed.strip_prefix("@@target") {
            let lang_str = rest.trim();
            let lang_token = lang_str.split_whitespace().next()?.trim();
            return TargetLanguage::try_from(lang_token).ok();
        }
        // Stop looking after first non-comment, non-empty line that isn't @@
        if !trimmed.starts_with("@@") {
            break;
        }
    }
    None
}

pub fn detect_header_target_annotation(content: &str) -> Option<TargetLanguage> {
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with("//") {
            continue;
        }
        if let Some(language) = parse_target_attribute(trimmed) {
            return Some(language);
        }
        if trimmed.starts_with('#') {
            continue;
        }
        break;
    }
    None
}

fn parse_target_attribute(line: &str) -> Option<TargetLanguage> {
    let inner = line.strip_prefix("#[")?.trim();
    let (body, _rest) = inner.split_once(']')?;
    let body = body.trim();
    let body = body.strip_prefix("target")?;
    let body = body.trim_start_matches(|c: char| c == ':' || c == '=' || c.is_whitespace());
    if body.is_empty() {
        return None;
    }
    let language_token = body.split_whitespace().next()?.trim();
    crate::frame_c::visitors::TargetLanguage::try_from(language_token).ok()
}

#[cfg(test)]
mod target_resolution_tests {
    //! FRAMEC_BUGS #36 — target precedence + the no-target warning.
    use super::{resolve_target_with_diag, TargetLanguage};

    const PRAGMA_RUST: &str = "@@[target(\"rust\")]\n@@system S { }";
    const NO_PRAGMA: &str = "@@system S { }";

    #[test]
    fn explicit_flag_wins_and_is_silent() {
        let (lang, diag) = resolve_target_with_diag(Some(TargetLanguage::Rust), NO_PRAGMA, None);
        assert_eq!(lang, TargetLanguage::Rust);
        assert!(diag.is_none());
    }

    #[test]
    fn flag_beats_pragma() {
        // -l python_3 on a file whose pragma says rust → flag wins, no warning.
        let (lang, diag) =
            resolve_target_with_diag(Some(TargetLanguage::Python3), PRAGMA_RUST, None);
        assert_eq!(lang, TargetLanguage::Python3);
        assert!(diag.is_none());
    }

    #[test]
    fn pragma_used_when_no_flag_and_is_silent() {
        let (lang, diag) = resolve_target_with_diag(None, PRAGMA_RUST, None);
        assert_eq!(lang, TargetLanguage::Rust);
        assert!(diag.is_none());
    }

    #[test]
    fn pragma_beats_env_default() {
        let (lang, diag) = resolve_target_with_diag(None, PRAGMA_RUST, Some("go".to_string()));
        assert_eq!(lang, TargetLanguage::Rust);
        assert!(diag.is_none());
    }

    #[test]
    fn valid_env_default_sets_target_and_silences() {
        let (lang, diag) = resolve_target_with_diag(None, NO_PRAGMA, Some("rust".to_string()));
        assert_eq!(lang, TargetLanguage::Rust);
        assert!(diag.is_none());
    }

    #[test]
    fn invalid_env_default_warns_and_falls_back() {
        let (lang, diag) = resolve_target_with_diag(None, NO_PRAGMA, Some("klingon".to_string()));
        assert_eq!(lang, TargetLanguage::Python3);
        let msg = diag.expect("invalid env should produce a diagnostic");
        assert!(msg.contains("FRAMEC_DEFAULT_TARGET"));
        assert!(msg.contains("klingon"));
    }

    #[test]
    fn no_signal_falls_back_to_python_with_warning() {
        let (lang, diag) = resolve_target_with_diag(None, NO_PRAGMA, None);
        assert_eq!(lang, TargetLanguage::Python3);
        let msg = diag.expect("true fallback should warn");
        assert!(msg.contains("no target specified"));
        assert!(msg.contains("FRAMEC_DEFAULT_TARGET"));
    }

    #[test]
    fn blank_env_default_is_ignored() {
        // FRAMEC_DEFAULT_TARGET="" must not count as a decision.
        let (lang, diag) = resolve_target_with_diag(None, NO_PRAGMA, Some("   ".to_string()));
        assert_eq!(lang, TargetLanguage::Python3);
        assert!(diag.is_some());
    }
}
