//! RFC-0055 §Migration / Go silent-drop defect — a persisted Go value that
//! `encoding/json` cannot serialise (an unsupported kind: chan/func/complex, an
//! unmarshalable Marshaler, a cyclic value) previously had its `json.Marshal`
//! error discarded (`jsonBytes, _ := ...`), producing a broken/empty blob with no
//! signal — silent data loss. The save now checks the error and fails LOUD (E753).
//! (The unexported-fields `{}` residual returns err == nil and is not detectable
//! within the Oceans boundary; it is an R2 documentation requirement, not a check.)

mod common;
use common::compile_source;

const SRC: &str = r#"
@@[persist(string)]
@@[save(save_state)]
@@[load(restore_state)]
@@system S {
    interface:
        noop()
    machine:
        $A { noop() {} }
    domain:
        marker: int = 1
}
"#;

#[test]
fn go_save_checks_marshal_error_and_fails_loud() {
    let c = compile_source(SRC, "go");
    assert!(
        c.contains("jsonBytes, __saveErr := json.Marshal(data)"),
        "[E753/go] save must capture the Marshal error, not discard it\n{c}"
    );
    assert!(
        c.contains(r#"if __saveErr != nil { panic("E753: persist save failed to serialize - "#),
        "[E753/go] save must panic loudly on a serialisation error\n{c}"
    );
    assert!(
        !c.contains("jsonBytes, _ := json.Marshal(data)"),
        "[E753/go] the error must not be discarded with `_`\n{c}"
    );
}
