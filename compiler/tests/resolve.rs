//! RESOLVE + VALIDATE — a symbol table built from the tree, and checks that cannot
//! read text.
//!
//! The old compiler's validator hand-rolled its own argument counter (`(` and `)` only,
//! blind to strings, chars, comments, `[]`, `{}`) while codegen used a *different*,
//! string-aware splitter. Two functions, one question, different answers — so framec
//! could **accept** one transition and **emit** a different one, and a state parameter
//! was silently left at its default. Exit 0. Wrong program.
//!
//! Here the validator has no bytes to count. It walks the tree.

use frame_compiler::resolve::{resolve, Severity, TypeRef};
use frame_compiler::scan::{literals::Target, segment};
use frame_compiler::validate::validate;
use frame_compiler::Source;

fn tree(text: &str, t: Target) -> frame_compiler::tree::FileAst {
    let src = Source::new("t.frm", text.as_bytes().to_vec()).unwrap();
    segment(&src, t).unwrap()
}

const DOOR: &str = r#"@@system Door {
    interface:
        open()
        close()
    machine:
        $Closed {
            $.attempts: int = 0
            open() { -> $Open }
        }
        $Open {
            close() { -> $Closed }
        }
    domain:
        log: string = ""
}
"#;

#[test]
fn the_symbol_table_is_built_from_the_tree() {
    let ast = tree(DOOR, Target::Python3);
    let (syms, diags) = resolve(&ast);

    assert!(diags.is_empty(), "{diags:?}");
    assert_eq!(syms.systems.len(), 1);

    let s = &syms.systems[0];
    assert_eq!(s.name, "Door");
    assert_eq!(
        s.interface.iter().map(|m| m.name.as_str()).collect::<Vec<_>>(),
        ["open", "close"]
    );
    assert_eq!(
        s.states.iter().map(|st| st.name.as_str()).collect::<Vec<_>>(),
        ["Closed", "Open"]
    );
    assert_eq!(s.domain.iter().map(|f| f.name.as_str()).collect::<Vec<_>>(), ["log"]);
    // The state var was found, on the state that owns it.
    assert_eq!(
        s.states[0].state_vars.iter().map(|v| v.name.as_str()).collect::<Vec<_>>(),
        ["attempts"]
    );
    // Handlers, per state.
    assert_eq!(s.states[0].handlers[0].event, "open");
    assert_eq!(s.states[1].handlers[0].event, "close");
}

/// E402 — a transition to a state that does not exist. Caught by walking the tree.
#[test]
fn a_transition_to_an_unknown_state_is_caught() {
    let bad = DOOR.replace("-> $Open", "-> $Ajar");
    let ast = tree(&bad, Target::Python3);
    let (syms, _) = resolve(&ast);
    let diags = validate(&ast, &syms);

    // Breaking the transition also leaves `$Open` unreachable, which is a (correct) W401
    // WARNING — but this test is about the E402 ERROR, so look only at errors.
    let errors: Vec<_> = diags
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .collect();
    assert_eq!(errors.len(), 1, "{diags:#?}");
    assert_eq!(errors[0].code, "E402");
    assert!(errors[0].message.contains("$Ajar"));
    assert!(
        errors[0].message.contains("$Closed") && errors[0].message.contains("$Open"),
        "the diagnostic should tell the user what the known states ARE"
    );
    // Every diagnostic carries a span. Always.
    assert!(errors[0].span.len() > 0);
}

/// W401 — a state no path reaches from the start state is dead code. A WARNING (not an
/// error), found by the dogfooded Reachability graph-walker wired into `validate()`.
#[test]
fn an_unreachable_state_is_a_w401_warning() {
    let src = r#"@@system Reach {
    interface:
        go()
    machine:
        $Start {
            go() { -> $Middle }
        }
        $Middle {
            go() { -> $Start }
        }
        $Orphan {
            go() { }
        }
}"#;
    let ast = tree(src, Target::Python3);
    let (syms, _) = resolve(&ast);
    let diags = validate(&ast, &syms);

    let w: Vec<_> = diags.iter().filter(|d| d.code == "W401").collect();
    assert_eq!(w.len(), 1, "exactly $Orphan is unreachable: {diags:#?}");
    assert_eq!(w[0].severity, Severity::Warning);
    assert!(w[0].message.contains("$Orphan"));
    assert!(w[0].message.contains("$Start"), "names the start state");
    // No ERRORS: an unreachable state does not make the program wrong.
    assert!(diags.iter().all(|d| d.severity != Severity::Error));
}

/// A machine whose every state is reachable from the start raises no W401 — including a
/// back-edge (a cycle is still reachable) and an HSM child whose parent stays live.
#[test]
fn a_fully_reachable_machine_is_clean() {
    let src = r#"@@system Line {
    interface:
        go()
    machine:
        $A {
            go() { -> $B }
        }
        $B {
            go() { -> $C }
        }
        $C {
            go() { -> $A }
        }
}"#;
    let ast = tree(src, Target::Python3);
    let (syms, _) = resolve(&ast);
    let diags = validate(&ast, &syms);
    assert!(
        diags.iter().all(|d| d.code != "W401"),
        "no state is dead: {diags:#?}"
    );
}

/// E814 — a persisted system MUST declare the full three-attribute contract. Bare
/// `@@[persist]` (no save/load method names) is rejected: framec will not invent the API.
#[test]
fn bare_persist_is_rejected_e814() {
    let src = r#"@@[persist]
@@system S {
    interface:
        go()
    machine:
        $A {
            go() { }
        }
    domain:
        n: int = 0
}"#;
    let ast = tree(src, Target::Python3);
    let (syms, diags) = resolve(&ast);
    let e814: Vec<_> = diags.iter().filter(|d| d.code == "E814").collect();
    assert_eq!(e814.len(), 1, "bare @@[persist] must be E814: {diags:#?}");
    assert_eq!(e814[0].severity, Severity::Error);
    assert!(syms.systems[0].persist.is_none(), "a rejected persist is not carried");
}

/// The three-attribute form is accepted and carries the user's chosen method names verbatim.
#[test]
fn the_three_attribute_persist_form_is_accepted() {
    let src = r#"@@[persist(str)]
@@[save(freeze)]
@@[load(thaw)]
@@system S {
    interface:
        go()
    machine:
        $A {
            go() { }
        }
    domain:
        n: int = 0
}"#;
    let ast = tree(src, Target::Python3);
    let (syms, diags) = resolve(&ast);
    assert!(diags.iter().all(|d| d.code != "E814"), "valid form must not E814: {diags:#?}");
    let p = syms.systems[0]
        .persist
        .as_ref()
        .expect("the persist contract is carried");
    assert_eq!(p.blob, "str");
    assert_eq!(p.save, "freeze");
    assert_eq!(p.load, "thaw");
}

/// **persist-reachability is a TRANSITIVE closure over embedded sub-systems.** The standing heir
/// of the migration-time `debug_assert` that guarded routing this closure onto the shipped
/// `@@system Reachability` (#219 single-source): a `@@[persist]` `Parent` embeds `Child`, which
/// embeds `Grand`, so all three ride inside the snapshot and MUST be persist-reachable (a
/// Rust/serde backend derives on exactly this set — `emit/rust.rs`); an unrelated `Lonely` must
/// NOT be. This pins both the multi-hop propagation the deleted hand fixpoint existed for AND the
/// negative (a declared-but-unembedded system stays off), which no prior fixture exercised.
#[test]
fn persist_reachability_closes_over_embedded_subsystems() {
    let src = r#"@@[persist(str)]
@@[save(freeze)]
@@[load(thaw)]
@@system Parent {
    interface:
        go()
    machine:
        $A { go() { } }
    domain:
        child: Child = @@Child()
}

@@system Child {
    interface:
        go()
    machine:
        $A { go() { } }
    domain:
        grand: Grand = @@Grand()
}

@@system Grand {
    interface:
        go()
    machine:
        $A { go() { } }
    domain:
        n: int = 0
}

@@system Lonely {
    interface:
        go()
    machine:
        $A { go() { } }
    domain:
        n: int = 0
}"#;
    let ast = tree(src, Target::Rust);
    let (syms, diags) = resolve(&ast);
    assert!(
        diags.iter().all(|d| d.severity != Severity::Error),
        "fixture must resolve without errors: {diags:#?}"
    );
    let pr = |name: &str| -> bool {
        syms.systems
            .iter()
            .find(|s| s.name == name)
            .unwrap_or_else(|| panic!("system `{name}` not found"))
            .persist_reachable
    };
    assert!(pr("Parent"), "the directly-persisted system is persist-reachable");
    assert!(
        pr("Child"),
        "a sub-system embedded by a persisted system is transitively persist-reachable"
    );
    assert!(
        pr("Grand"),
        "persist-reachability is a CLOSURE — two hops deep is still reachable"
    );
    assert!(
        !pr("Lonely"),
        "a system embedded by no persisted system is NOT persist-reachable (the negative)"
    );
}

/// E730 — `@@system public S` is redundant (systems are public by default) and rejected. The
/// real name still resolves to `S`, not the modifier keyword.
#[test]
fn redundant_public_is_rejected_e730() {
    let src = r#"@@system public S {
    interface:
        go()
    machine:
        $A { go() { } }
}"#;
    let ast = tree(src, Target::Java);
    let (syms, diags) = resolve(&ast);
    let e730: Vec<_> = diags.iter().filter(|d| d.code == "E730").collect();
    assert_eq!(e730.len(), 1, "@@system public must be E730: {diags:#?}");
    assert_eq!(e730[0].severity, Severity::Error);
    assert_eq!(syms.systems[0].name, "S", "the name is S, not the `public` keyword");
    assert!(!syms.systems[0].private, "public is not private");
}

/// `@@system private S` is NOT E730 — reducing visibility is legitimate; only the redundant
/// `public` is rejected. (Whether `private` is realizable is the separate per-target E731.)
#[test]
fn private_is_not_redundant_public_e730() {
    let src = r#"@@system private S {
    interface:
        go()
    machine:
        $A { go() { } }
}"#;
    let ast = tree(src, Target::Java);
    let (syms, diags) = resolve(&ast);
    assert!(diags.iter().all(|d| d.code != "E730"), "private is not redundant: {diags:#?}");
    assert!(syms.systems[0].private, "private is carried");
    assert_eq!(syms.systems[0].name, "S", "the name is S, not the `private` keyword");
}

/// **The types are the user's. framec carries them and does not look inside.**
#[test]
fn a_type_is_opaque_text() {
    let text = r#"@@system S {
    interface:
        go()
    machine:
        $A { go() { } }
    domain:
        weird: Rc<RefCell<HashMap<String, Vec<u8>>>> = def()
}
"#;
    let ast = tree(text, Target::Rust);
    let (syms, diags) = resolve(&ast);
    assert!(diags.is_empty(), "an unknown type is not an error — {diags:?}");
    assert_eq!(
        syms.systems[0].domain[0].ty,
        TypeRef::Opaque("Rc<RefCell<HashMap<String, Vec<u8>>>>".into()),
        "framec must carry the user's type VERBATIM and know nothing about it"
    );
}

/// Exact-name resolution: `kid: Child` resolves to the system `Child`.
#[test]
fn a_field_typed_with_a_system_name_resolves_to_that_system() {
    let text = r#"@@system Child {
    interface:
        go()
    machine:
        $A { go() { } }
}

@@system Parent {
    interface:
        go()
    machine:
        $A { go() { } }
    domain:
        kid: Child = @@Child()
}
"#;
    let ast = tree(text, Target::Python3);
    let (syms, diags) = resolve(&ast);
    assert!(diags.is_empty(), "{diags:?}");
    let parent = syms.systems.iter().find(|s| s.name == "Parent").unwrap();
    assert_eq!(parent.domain[0].ty, TypeRef::System("Child".into()));
}

/// **A WRAPPED system type with NO `@@` initializer is a DIAGNOSTIC, not a guess.**
///
/// `Rc<RefCell<Child>>` mentions `Child`, but `= mk()` tells framec nothing. So framec
/// suspects, cannot know, and says so. Resolving it would require parsing sixteen
/// wrapper grammars — the "never parse the user's code" rule broken one level up.
/// Silently shrugging would break cross-file persist.
///
/// Contrast the next test: with `= @@Child()`, there is nothing to guess about.
#[test]
fn a_wrapped_system_type_is_reported_not_guessed() {
    let text = r#"@@system Child {
    interface:
        go()
    machine:
        $A { go() { } }
}

@@system Parent {
    interface:
        go()
    machine:
        $A { go() { } }
    domain:
        kid: Rc<RefCell<Child>> = mk()
}
"#;
    let ast = tree(text, Target::Rust);
    let (syms, diags) = resolve(&ast);

    assert_eq!(diags.len(), 1, "{diags:#?}");
    assert_eq!(diags[0].code, "E640");
    assert!(diags[0].message.contains("Rc<RefCell<Child>>"));
    assert!(
        diags[0].message.contains("= @@Child("),
        "the diagnostic must tell the user what to DO — and the thing to do is to use \
         FRAME's own syntax (`= @@Child()`), which settles the question without framec \
         ever reading the type"
    );

    let parent = syms.systems.iter().find(|s| s.name == "Parent").unwrap();
    assert!(matches!(
        parent.domain[0].ty,
        TypeRef::WrappedSystem { .. }
    ));
}

/// And it must not fire on a name that merely *contains* a system name.
/// `ChildProcess` is not `Child`.
#[test]
fn a_substring_is_not_a_mention() {
    let text = r#"@@system Child {
    interface:
        go()
    machine:
        $A { go() { } }
}

@@system Parent {
    interface:
        go()
    machine:
        $A { go() { } }
    domain:
        p: ChildProcess = mk()
        q: GrandChild = mk()
}
"#;
    let ast = tree(text, Target::Rust);
    let (_syms, diags) = resolve(&ast);
    assert!(
        diags.is_empty(),
        "`ChildProcess` and `GrandChild` are NOT the system `Child` — {diags:#?}"
    );
}

/// A field may be typed with a system declared LATER in the file. Resolution must not
/// depend on declaration order — that is a footgun nobody expects from a compiler.
#[test]
fn resolution_does_not_depend_on_declaration_order() {
    let text = r#"@@system Parent {
    interface:
        go()
    machine:
        $A { go() { } }
    domain:
        kid: Child = @@Child()
}

@@system Child {
    interface:
        go()
    machine:
        $A { go() { } }
}
"#;
    let ast = tree(text, Target::Python3);
    let (syms, _) = resolve(&ast);
    let parent = syms.systems.iter().find(|s| s.name == "Parent").unwrap();
    assert_eq!(
        parent.domain[0].ty,
        TypeRef::System("Child".into()),
        "`Child` is declared after `Parent` and must still resolve"
    );
}

/// The whole corpus: the symbol table builds, and nothing spuriously errors.
#[test]
fn the_corpus_resolves_cleanly() {
    let mut systems = 0usize;
    let mut states = 0usize;
    let mut handlers = 0usize;
    let mut noise = Vec::new();

    for (path, target) in crate_corpus() {
        let bytes = std::fs::read(&path).unwrap();
        let src = Source::new(path.to_string_lossy(), bytes).unwrap();
        let Ok(ast) = segment(&src, target) else { continue };
        let (syms, diags) = resolve(&ast);
        let v = validate(&ast, &syms);

        systems += syms.systems.len();
        for s in &syms.systems {
            states += s.states.len();
            handlers += s.states.iter().map(|st| st.handlers.len()).sum::<usize>();
        }
        // "Cleanly" means no ERRORS — the compiler must not *reject* a valid spec. Warnings
        // (e.g. W401 for a fixture with a deliberately dead substate) are informational and do
        // not count as noise.
        for d in diags.iter().chain(v.iter()).filter(|d| d.severity == Severity::Error) {
            noise.push(format!("{}: {} {}", path.display(), d.code, d.message.lines().next().unwrap_or("")));
        }
    }

    eprintln!("\n  SYMBOL TABLE over the corpus");
    eprintln!("  ----------------------------");
    eprintln!("  systems  {systems}");
    eprintln!("  states   {states}");
    eprintln!("  handlers {handlers}\n");

    assert!(systems > 200, "the corpus must yield systems, got {systems}");
    assert!(
        noise.is_empty(),
        "\nthe EXISTING corpus must resolve and validate cleanly — a rebuilt compiler \
         that rejects the specification is wrong, not the specification.\n\n{}\n",
        noise.iter().take(15).map(|n| format!("  {n}")).collect::<Vec<_>>().join("\n")
    );
}

// The corpus loader (see totality.rs for the full version and why it panics on an
// unclassified directory rather than silently skipping it).
fn crate_corpus() -> Vec<(std::path::PathBuf, Target)> {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("framec/tests/fixtures");
    let mut out = Vec::new();
    for d in std::fs::read_dir(&root).unwrap().flatten() {
        let name = d.file_name().to_string_lossy().into_owned();
        if name == "erlang" {
            continue; // deprecated and removed
        }
        let n = match name.as_str() {
            "python" | "python_3" | "_canonical" => "python",
            other => other,
        };
        let Some(t) = Target::ALL.iter().find(|t| t.name() == n) else {
            continue;
        };
        for f in std::fs::read_dir(d.path()).unwrap().flatten() {
            if f.path().extension().map(|e| e == "frm").unwrap_or(false) {
                out.push((f.path(), *t));
            }
        }
    }
    out
}


/// **The corpus's own case, and the reason the design changed.**
///
/// `inner: Inner* = @@Inner()` — C's MANDATORY spelling for a system instance (C has no
/// references; `create` returns a pointer). Telling that user to write `Inner` would be
/// telling them to write something that is not C.
///
/// But `@@Inner()` is FRAME's syntax. framec knows the field holds a system without
/// reading the type at all. It was reading the user's text to recover a fact its own
/// text already stated — RULE 1, violated, in the compiler we were building to enforce
/// RULE 1.
#[test]
fn a_frame_initializer_settles_the_question_whatever_the_type_spelling() {
    for spelling in [
        "Inner*",                    // C
        "Rc<RefCell<Inner>>",        // Rust
        "std::shared_ptr<Inner>",    // C++
        "Optional[Inner]",           // Python
        "Inner?",                    // Swift / Kotlin
    ] {
        let text = format!(
            r#"@@system Inner {{
    interface:
        go()
    machine:
        $A {{ go() {{ }} }}
}}

@@system Outer {{
    interface:
        go()
    machine:
        $A {{ go() {{ }} }}
    domain:
        inner: {spelling} = @@Inner()
}}
"#
        );
        let ast = tree(&text, Target::Rust);
        let (syms, diags) = resolve(&ast);
        assert!(
            diags.is_empty(),
            "`{spelling}` with an `= @@Inner()` initializer needs NO diagnostic — \
             Frame's own syntax already said what it is. Got: {diags:#?}"
        );
        let outer = syms.systems.iter().find(|s| s.name == "Outer").unwrap();
        assert_eq!(
            outer.domain[0].ty,
            TypeRef::System("Inner".into()),
            "`{spelling}` must resolve to the system Inner via the initializer, \
             WITHOUT framec parsing the type"
        );
    }
}
