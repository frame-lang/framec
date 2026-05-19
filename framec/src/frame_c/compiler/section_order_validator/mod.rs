//! E113 section-order validator — Frame multi-state system.
//!
//! RFC-0035 round 4. This is the round where Frame's natural
//! shape (state machine with transitions) actually fits the
//! problem. Section-order validation has two phases:
//!
//!   $Walking     — accept sections in canonical order, tracking
//!                  the highest section index seen.
//!   $OutOfOrder  — terminal; further sections are absorbed
//!                  (E113 is reported once per system).
//!
//! The transition $Walking → $OutOfOrder fires on the first
//! out-of-order section. That is exactly the "error-absorbing
//! terminal state" pattern Frame state machines were designed
//! for. The match is so clean that section-order validation
//! reads as a state machine on the page — no awkward shoehorning
//! needed.
//!
//! Public API:
//!   `validate_section_order(kinds: &[SystemSectionKind]) -> Option<String>`
//!
//! Returns `Some(error_message)` for the first out-of-order
//! section, or `None` if the sequence is in order. The caller
//! (validator/structure.rs) wraps the message in a
//! `ValidationError` with code E113.
//!
//! To regenerate after editing the `.frs` source:
//!   ./target/release/framec compile -l rust \
//!     framec/src/frame_c/compiler/section_order_validator/section_order_validator.frs \
//!     > framec/src/frame_c/compiler/section_order_validator/section_order_validator.gen.rs

use crate::frame_c::compiler::frame_ast::SystemSectionKind;

mod section_order_validator_fsm {
    #![allow(unreachable_patterns)]
    #![allow(unused_mut)]
    #![allow(dead_code)]
    #![allow(non_snake_case)]
    #![allow(unused_variables)]
    include!("section_order_validator.gen.rs");
}

fn kind_name(k: SystemSectionKind) -> &'static str {
    match k {
        SystemSectionKind::Operations => "Operations",
        SystemSectionKind::Interface => "Interface",
        SystemSectionKind::Machine => "Machine",
        SystemSectionKind::Actions => "Actions",
        SystemSectionKind::Domain => "Domain",
    }
}

/// Walk `kinds` in order and return the E113 message for the
/// first out-of-order section, or `None` if the sequence is in
/// canonical order.
///
/// The walk routes through a Frame FSM that transitions
/// $Walking → $OutOfOrder on the first violation. Once in
/// $OutOfOrder the FSM absorbs further check() calls
/// (matching the validator's "report once per system"
/// contract).
pub(crate) fn validate_section_order(kinds: &[SystemSectionKind]) -> Option<String> {
    let mut validator = section_order_validator_fsm::SectionOrderValidator::__create();
    for k in kinds {
        let result = validator.check(kind_name(*k).to_string());
        if let Some(rest) = result.strip_prefix("E113|") {
            return Some(rest.to_string());
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_in_order() {
        assert_eq!(validate_section_order(&[]), None);
    }

    #[test]
    fn full_canonical_order_ok() {
        let kinds = [
            SystemSectionKind::Operations,
            SystemSectionKind::Interface,
            SystemSectionKind::Machine,
            SystemSectionKind::Actions,
            SystemSectionKind::Domain,
        ];
        assert_eq!(validate_section_order(&kinds), None);
    }

    #[test]
    fn partial_order_ok() {
        let kinds = [SystemSectionKind::Interface, SystemSectionKind::Machine];
        assert_eq!(validate_section_order(&kinds), None);
    }

    #[test]
    fn domain_before_machine_detected() {
        let kinds = [SystemSectionKind::Domain, SystemSectionKind::Machine];
        let err = validate_section_order(&kinds).expect("expected E113");
        assert!(err.contains("blocks out of order"));
    }

    #[test]
    fn interface_after_actions_detected() {
        let kinds = [SystemSectionKind::Actions, SystemSectionKind::Interface];
        let err = validate_section_order(&kinds).expect("expected E113");
        assert!(err.contains("blocks out of order"));
    }

    #[test]
    fn report_once_only() {
        // Once the FSM lands in $OutOfOrder, subsequent sections
        // (even further out-of-order ones) don't re-fire. The
        // validate_section_order helper returns the FIRST
        // out-of-order section's message and stops.
        let kinds = [
            SystemSectionKind::Domain,
            SystemSectionKind::Operations,
            SystemSectionKind::Interface,
        ];
        let err = validate_section_order(&kinds).expect("expected E113");
        assert!(err.contains("blocks out of order"));
    }
}
