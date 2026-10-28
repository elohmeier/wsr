//! Regressions for false passes in the shared original-executable comparator.
use wsr_core::pb::real::Ext;

mod diff;
use diff::{Compare, layout};
use wsr_core::state::Globals;

fn field(name: &str) -> (Compare, String) {
    let mut compare = Compare::default();
    let mut key = None;
    for field in layout() {
        if field.name == name {
            key = Some(field.key);
        } else {
            compare = compare.ignore(&field.name);
        }
    }
    (compare, key.expect("unknown field"))
}

#[test]
fn integers_and_currency_compare_exactly_above_double_precision() {
    for name in ["MaxBondIssue", "MergerPricePerShare"] {
        let (compare, key) = field(name);
        let a = format!("{{\"{key}\":9007199254740992}}");
        let b = format!("{{\"{key}\":9007199254740993}}");
        assert_eq!(compare.diffs_json(&a, &b).len(), 1);
        assert!(compare.diffs_json(&a, &a).is_empty());
        assert!(
            compare
                .diffs_json(
                    &format!("{{\"{key}\":5}}"),
                    &format!("{{\"{key}\":5.00e0}}")
                )
                .is_empty()
        );
        assert_eq!(
            compare
                .diffs_json(
                    &format!("{{\"{key}\":5.01}}"),
                    &format!("{{\"{key}\":5.01}}")
                )
                .len(),
            1
        );
    }
}

#[test]
fn nonfinite_values_cannot_pass_even_when_both_peers_agree() {
    let (compare, key) = field("TmpExt1");
    for value in ["null", "1e999"] {
        let json = format!("{{\"{key}\":{value}}}");
        assert_eq!(compare.diffs_json(&json, &json).len(), 1);
    }
    let mut input = Globals::default();
    input.tmp_ext1 = Ext::NAN;
    let compare = Compare::default().allow_nonfinite_from(&input);
    assert!(compare.diffs(&input, &input).is_empty());
    let mut after = input.clone();
    after.tmp_ext2 = Ext::NAN;
    assert_eq!(
        compare.diffs(&after, &after).len(),
        1,
        "exemption expanded to a new cell"
    );
}

#[test]
fn array_shapes_and_missing_fields_cannot_pass() {
    let (compare, key) = field("BankLargestDebtor");
    let short = format!("{{\"{key}\":[0]}}");
    assert_eq!(compare.diffs_json(&short, &short).len(), 1);
    assert_eq!(compare.diffs_json("{}", "{}").len(), 1);
    let scalar = format!("{{\"{key}\":0}}");
    assert_eq!(compare.diffs_json(&scalar, &scalar).len(), 1);
    let mut input = Globals::default();
    input.cash[1] = Ext::NAN;
    let (mut compare, key) = field("Cash");
    compare = compare.allow_nonfinite_from(&input);
    let scalar = format!("{{\"{key}\":null}}");
    assert_eq!(
        compare.diffs_json(&scalar, &scalar).len(),
        1,
        "an exemption must not hide a malformed array"
    );
}

#[test]
fn matching_invalid_field_types_cannot_pass() {
    for name in ["TmpExt1", "MaxBondIssue", "ClockState"] {
        let (compare, key) = field(name);
        for value in ["null", "{}", "true"] {
            let json = format!("{{\"{key}\":{value}}}");
            assert_eq!(compare.diffs_json(&json, &json).len(), 1, "{name}: {value}");
        }
    }
}

#[test]
fn floating_tolerance_does_not_hide_strings() {
    let (compare, key) = field("TmpExt1");
    assert!(
        compare
            .diffs_json(
                &format!("{{\"{key}\":1}}"),
                &format!("{{\"{key}\":1.0000000000001}}")
            )
            .is_empty()
    );
    let (compare, key) = field("ClockState");
    assert_eq!(
        compare
            .diffs_json(
                &format!("{{\"{key}\":\"RUN\"}}"),
                &format!("{{\"{key}\":\"STOP\"}}")
            )
            .len(),
        1
    );
}

#[test]
fn raw_extended_values_compare_without_double_rounding_or_overflow() {
    let (mut compare, key) = field("TmpExt1");
    compare.exact_ext = true;
    let a = format!("{{\"{key}\":\"x87:3fff8000000000000000\"}}");
    let b = format!("{{\"{key}\":\"x87:3fff8000000000000001\"}}");
    assert_eq!(compare.diffs_json(&a, &b).len(), 1);
    compare.exact_ext = false;
    assert!(compare.diffs_json(&a, &b).is_empty());
    let huge = format!("{{\"{key}\":\"x87:7ffe8000000000000000\"}}");
    assert!(compare.diffs_json(&huge, &huge).is_empty());
    let nan = format!("{{\"{key}\":\"x87:7fffc000000000000001\"}}");
    assert_eq!(compare.diffs_json(&nan, &nan).len(), 1);
}
