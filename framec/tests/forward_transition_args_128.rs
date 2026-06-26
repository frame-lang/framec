//! Issue #128 — forward transition `-> => $State` must carry arg
//! decorations (state args, enter args, exit args) and must NOT
//! falsely flag the target unreachable (W414).
//!
//! Bugs fixed (forward decorations were dropped at every layer):
//!   1. **Detection** (`lexer/frame_stmt.rs` + `native_region_scanner/
//!      unified.rs`): the `=>` forward marker and enter-args paren were
//!      only recognized in the bare `-> => $S` shape. With enter/exit
//!      decorations the `=>` was stranded, so the transition was
//!      misclassified (the trailing `=> $S` re-lexed as a bare
//!      event-forward → E403, or the target flagged W414). Both scanners
//!      now accept `=>` and enter args in either order.
//!   2. **Parser** (`pipeline_parser/mod.rs`): the forward arms now
//!      absorb the lexer's reordered NativeCode decoration tokens and
//!      still emit ONE `Transition` with `is_forward`, so scanner
//!      enrichment pairs by source order and the W414 walker sees the edge.
//!   3. **Codegen** (`frame_expansion/transition.rs` + `rust_system.rs`):
//!      the separate forward branch emitted empty arg channels. It is
//!      deleted; the forward variant now runs through the SAME regular
//!      branch (shared exit/enter/state arg emission) and only adds the
//!      `forward_event` set (via the `forward_event_line` closure).
//!   4. **Validation** (`frame_validator/arcanum_checks.rs`): the legacy
//!      arity check false-positived E405 on a decorated forward (its args
//!      live in the metadata, not the positional vec). It now defers to
//!      the precise `transitions.rs` E405/E417 checks when decorations
//!      are present.

mod common;
use common::compile_with_warnings;

/// A system whose only path into `$S`/`$T`/`$U`/`$V` is via a decorated
/// forward transition. If the parser drops the statement, these states
/// are unreachable → W414.
const DECORATED_FORWARD_SRC: &str = r#"
@@[target("python_3")]
@@system ForwardArgs {
    interface:
        go_state()
        go_enter()
        go_exit()
        go_combo()

    machine:
        $Begin {
            go_state() { -> => $S(7) }
            go_enter() { -> (11) => $T }
            go_exit()  { (13) -> => $U }
            go_combo() { (17) -> (19) => $V(23) }
            <$(x) { }
        }

        $S(p) {
            $>() { }
            go_state() { }
        }

        $T {
            $>(e) { }
            go_enter() { }
        }

        $U {
            $>() { }
            go_exit() { }
        }

        $V(s) {
            $>(e) { }
            go_combo() { }
        }
}
"#;

#[test]
fn decorated_forward_does_not_emit_w414() {
    let (_code, warnings) = compile_with_warnings(DECORATED_FORWARD_SRC, "python_3");
    let w414: Vec<&String> = warnings.iter().filter(|w| w.contains("W414")).collect();
    assert!(
        w414.is_empty(),
        "decorated forward targets must be reachable; got W414: {w414:?}"
    );
}

#[test]
fn decorated_forward_emits_state_args_python() {
    let (code, _w) = compile_with_warnings(DECORATED_FORWARD_SRC, "python_3");
    // `-> => $S(7)` must carry the state arg into __prepareEnter.
    assert!(
        code.contains("__prepareEnter(\"S\", [7], [])"),
        "forward state-arg must land in __prepareEnter; got:\n{code}"
    );
}

#[test]
fn decorated_forward_emits_enter_args_python() {
    let (code, _w) = compile_with_warnings(DECORATED_FORWARD_SRC, "python_3");
    // `-> (11) => $T` must carry the enter arg into __prepareEnter.
    assert!(
        code.contains("__prepareEnter(\"T\", [], [11])"),
        "forward enter-arg must land in __prepareEnter; got:\n{code}"
    );
}

#[test]
fn decorated_forward_emits_exit_args_python() {
    let (code, _w) = compile_with_warnings(DECORATED_FORWARD_SRC, "python_3");
    // `(13) -> => $U` must populate exit args on the source chain.
    assert!(
        code.contains("__prepareExit([13])"),
        "forward exit-arg must call __prepareExit; got:\n{code}"
    );
}

#[test]
fn decorated_forward_emits_combined_args_python() {
    let (code, _w) = compile_with_warnings(DECORATED_FORWARD_SRC, "python_3");
    // `(17) -> (19) => $V(23)` must populate all three channels.
    assert!(
        code.contains("__prepareExit([17])"),
        "combined forward exit-arg missing; got:\n{code}"
    );
    assert!(
        code.contains("__prepareEnter(\"V\", [23], [19])"),
        "combined forward state+enter args missing; got:\n{code}"
    );
}

#[test]
fn decorated_forward_still_sets_forward_event_python() {
    let (code, _w) = compile_with_warnings(DECORATED_FORWARD_SRC, "python_3");
    // The defining property of a forward transition: the current event
    // is re-dispatched into the target after enter completes.
    assert!(
        code.contains("forward_event = __e"),
        "forward transition must still set forward_event; got:\n{code}"
    );
}

/// Typed variant: statically-typed targets (Rust/Java/C#/C++/Go) reject
/// untyped params with E606 (Frame has no type system). Type names pass
/// through verbatim, so `: int` satisfies the annotation requirement
/// without the validator judging the type string.
const DECORATED_FORWARD_TYPED_SRC: &str = r#"
@@[target("rust")]
@@system ForwardArgs {
    interface:
        go_state()
        go_enter()
        go_exit()
        go_combo()

    machine:
        $Begin {
            go_state() { -> => $S(7) }
            go_enter() { -> (11) => $T }
            go_exit()  { (13) -> => $U }
            go_combo() { (17) -> (19) => $V(23) }
            <$(x: int) { }
        }

        $S(p: int) {
            $>() { }
            go_state() { }
        }

        $T {
            $>(e: int) { }
            go_enter() { }
        }

        $U {
            $>() { }
            go_exit() { }
        }

        $V(s: int) {
            $>(e: int) { }
            go_combo() { }
        }
}
"#;

/// Transpile-clean (no W414) across a spread of backends — both the
/// dynamic ones (untyped fixture) and the statically-typed ones (typed
/// fixture). Every backend's forward branch now shares the regular
/// branch's arg emission (#128).
#[test]
fn decorated_forward_transpiles_clean_across_backends() {
    // Dynamic targets accept untyped params.
    for target in ["typescript", "javascript", "ruby", "lua", "php", "dart"] {
        let src = DECORATED_FORWARD_SRC.replace("python_3", target);
        let (_code, warnings) = compile_with_warnings(&src, target);
        let w414: Vec<&String> = warnings.iter().filter(|w| w.contains("W414")).collect();
        assert!(
            w414.is_empty(),
            "[{target}] decorated forward must not emit W414: {w414:?}"
        );
    }
    // Statically-typed targets need a type annotation (E606).
    for target in ["rust", "java", "csharp", "cpp", "go", "kotlin", "swift"] {
        let src = DECORATED_FORWARD_TYPED_SRC.replace("rust", target);
        let (_code, warnings) = compile_with_warnings(&src, target);
        let w414: Vec<&String> = warnings.iter().filter(|w| w.contains("W414")).collect();
        assert!(
            w414.is_empty(),
            "[{target}] decorated forward must not emit W414: {w414:?}"
        );
    }
}
