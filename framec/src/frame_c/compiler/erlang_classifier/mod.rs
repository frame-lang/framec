//! Erlang per-line native rewriter — Frame-generated classifier.
//!
//! RFC-0035 round 3. The Erlang body processor walks the spliced
//! handler text and asks this classifier to tag + rewrite each
//! line as one of seven `ErlangRewrite` variants. The
//! classification logic itself lives in
//! `erlang_line_classifier.frs`; this module exposes the typed
//! public API (`erlang_rewrite_native_classified` /
//! `erlang_rewrite_native_classified_full`) and parses the
//! FSM's pipe-delimited tagged string back into the typed
//! variant the body processor expects.
//!
//! Round 3 was deliberately the "awkward fit" case to dogfood.
//! The classifier:
//!
//! - Has a rich return type (7 variants, several with struct
//!   payloads). Frame interface return types are `String`; the
//!   FSM serializes to a tagged string and the glue parses back.
//! - Takes two `&[String]` slice parameters that have to be
//!   flattened to comma-separated strings at the boundary.
//! - The body is mostly straight-line if/else branches that
//!   could equally well live as a free Rust function.
//!
//! These observations are part of the RFC-0035 record. The
//! migration still lands the function as a `.frs` because
//! dogfooding Frame on its own internals — including on shapes
//! Frame doesn't fit cleanly — is the explicit goal of the RFC.
//!
//! To regenerate after editing the `.frs` source:
//!   ./target/release/framec compile -l rust \
//!     framec/src/frame_c/compiler/erlang_classifier/erlang_line_classifier.frs \
//!     > framec/src/frame_c/compiler/erlang_classifier/erlang_line_classifier.gen.rs

mod erlang_line_classifier_fsm {
    #![allow(unreachable_patterns)]
    #![allow(unused_mut)]
    #![allow(dead_code)]
    #![allow(non_snake_case)]
    #![allow(unused_variables)]
    include!("erlang_line_classifier.gen.rs");
}

/// Result of rewriting a line of native code for Erlang.
/// Mirrors the shape the body processor expects (was previously
/// defined inside `erlang_system/native_rewrite.rs`).
pub(crate) enum ErlangRewrite {
    ActionCall(String),
    ActionCallWithBind { field: String, call: String },
    InterfaceCallWithBind {
        field: String,
        method: String,
        args: String,
    },
    RecordUpdate {
        field: String,
        value: String,
    },
    InterfaceCall {
        method: String,
        args: String,
        result_var: String,
    },
    Plain(String),
    #[allow(dead_code)]
    Reply(String),
}

/// Rewrite a line of native code for Erlang, classifying the
/// result. Two-arity wrapper used at call sites that haven't
/// been threaded with the interface-names list yet.
pub(crate) fn erlang_rewrite_native_classified(
    line: &str,
    action_names: &[String],
    data_var: &str,
) -> ErlangRewrite {
    erlang_rewrite_native_classified_full(line, action_names, &[], data_var)
}

/// Full-arity classifier with interface-names awareness.
pub(crate) fn erlang_rewrite_native_classified_full(
    line: &str,
    action_names: &[String],
    interface_names: &[String],
    data_var: &str,
) -> ErlangRewrite {
    let actions_csv = action_names.join(",");
    let interfaces_csv = interface_names.join(",");
    let encoded = erlang_line_classifier_fsm::ErlangLineClassifier::__create().classify(
        line.to_string(),
        actions_csv,
        interfaces_csv,
        data_var.to_string(),
    );
    parse_encoded(&encoded)
}

fn parse_encoded(encoded: &str) -> ErlangRewrite {
    let (tag, rest) = match encoded.find('|') {
        Some(i) => (&encoded[..i], &encoded[i + 1..]),
        None => (encoded, ""),
    };
    match tag {
        "InterfaceCallWithBind" => {
            let (field, after_field) = take_field(rest, "field=");
            let (method, after_method) = take_field(after_field, "method=");
            let args = strip_prefix_or_all(after_method, "args=");
            ErlangRewrite::InterfaceCallWithBind {
                field,
                method,
                args,
            }
        }
        "InterfaceCall" => {
            let (method, after_method) = take_field(rest, "method=");
            let (args, after_args) = take_field_value_may_contain_pipe(after_method, "args=", "|result_var=");
            let result_var = strip_prefix_or_all(after_args, "result_var=");
            ErlangRewrite::InterfaceCall {
                method,
                args,
                result_var,
            }
        }
        "ActionCallWithBind" => {
            let (field, after_field) = take_field(rest, "field=");
            let call = strip_prefix_or_all(after_field, "call=");
            ErlangRewrite::ActionCallWithBind { field, call }
        }
        "ActionCall" => ErlangRewrite::ActionCall(rest.to_string()),
        "RecordUpdate" => {
            let (field, after_field) = take_field(rest, "field=");
            let value = strip_prefix_or_all(after_field, "value=");
            ErlangRewrite::RecordUpdate { field, value }
        }
        "Plain" => ErlangRewrite::Plain(rest.to_string()),
        _ => ErlangRewrite::Plain(encoded.to_string()),
    }
}

/// Strip the leading `<key>` prefix, then read up to the next
/// `|` boundary. Returns the field value and the remainder
/// (starting AFTER the `|`).
fn take_field<'a>(s: &'a str, key: &str) -> (String, &'a str) {
    let after_key = s.strip_prefix(key).unwrap_or(s);
    match after_key.find('|') {
        Some(i) => (after_key[..i].to_string(), &after_key[i + 1..]),
        None => (after_key.to_string(), ""),
    }
}

/// Like `take_field`, but reads the value up to the literal
/// `next_boundary` rather than the first `|` — so the value may
/// itself legitimately contain `|`.
fn take_field_value_may_contain_pipe<'a>(
    s: &'a str,
    key: &str,
    next_boundary: &str,
) -> (String, &'a str) {
    let after_key = s.strip_prefix(key).unwrap_or(s);
    match after_key.find(next_boundary) {
        Some(i) => (after_key[..i].to_string(), &after_key[i + 1..]),
        None => (after_key.to_string(), ""),
    }
}

fn strip_prefix_or_all(s: &str, key: &str) -> String {
    s.strip_prefix(key).unwrap_or(s).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn match_action_call(r: ErlangRewrite, expected_text: &str) {
        if let ErlangRewrite::ActionCall(text) = r {
            assert_eq!(text, expected_text);
        } else {
            panic!("expected ActionCall");
        }
    }

    fn match_plain(r: ErlangRewrite, expected_text: &str) {
        if let ErlangRewrite::Plain(text) = r {
            assert_eq!(text, expected_text);
        } else {
            panic!("expected Plain");
        }
    }

    #[test]
    fn plain_line_passthrough() {
        let r = erlang_rewrite_native_classified("X = 1", &[], "Data");
        match_plain(r, "X = 1");
    }

    #[test]
    fn self_field_access_to_record_lookup() {
        let r = erlang_rewrite_native_classified("X = self.counter", &[], "Data");
        match_plain(r, "X = Data#data.counter");
    }

    #[test]
    fn action_call_threads_data() {
        let r = erlang_rewrite_native_classified(
            "self.tick(5)",
            &["tick".to_string()],
            "Data",
        );
        match_action_call(r, "tick(Data, 5)");
    }

    #[test]
    fn record_update_emits_variant() {
        let r = erlang_rewrite_native_classified("self.counter = 7", &[], "Data");
        if let ErlangRewrite::RecordUpdate { field, value } = r {
            assert_eq!(field, "counter");
            assert_eq!(value, "7");
        } else {
            panic!("expected RecordUpdate");
        }
    }

    #[test]
    fn interface_call_emits_variant() {
        let r = erlang_rewrite_native_classified_full(
            "self.echo(X)",
            &[],
            &["echo".to_string()],
            "Data",
        );
        if let ErlangRewrite::InterfaceCall {
            method,
            args,
            result_var,
        } = r
        {
            assert_eq!(method, "echo");
            assert_eq!(args, "X");
            assert_eq!(result_var, "_");
        } else {
            panic!("expected InterfaceCall");
        }
    }
}
