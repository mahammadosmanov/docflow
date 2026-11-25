use std::collections::BTreeMap;

use super::*;

fn values(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
    pairs
        .iter()
        .map(|(key, value)| ((*key).to_string(), (*value).to_string()))
        .collect()
}

fn compute(op: Op, inputs: &[&str]) -> Compute {
    Compute {
        op,
        inputs: inputs.iter().map(|key| (*key).to_string()).collect(),
        ..Compute::default()
    }
}

#[test]
fn sum_and_difference_keep_the_decimals_of_their_inputs() {
    let v = values(&[("a", "1.5"), ("b", "2.25"), ("c", "10")]);
    assert_eq!(
        compute(Op::Sum, &["a", "b", "c"])
            .evaluate("T", &v)
            .unwrap(),
        "13.75"
    );
    assert_eq!(
        compute(Op::Difference, &["c", "a"])
            .evaluate("T", &v)
            .unwrap(),
        "8.5"
    );
    assert_eq!(
        compute(Op::Sum, &["c", "c"]).evaluate("T", &v).unwrap(),
        "20"
    );
    // 0.1 + 0.2 must not show float noise.
    let v = values(&[("a", "0.1"), ("b", "0.2")]);
    assert_eq!(
        compute(Op::Sum, &["a", "b"]).evaluate("T", &v).unwrap(),
        "0.3"
    );
}

#[test]
fn a_comma_in_an_input_makes_the_result_use_a_comma() {
    let v = values(&[("a", "1,5"), ("b", "2")]);
    assert_eq!(
        compute(Op::Sum, &["a", "b"]).evaluate("T", &v).unwrap(),
        "3,5"
    );
}

#[test]
fn offset_adds_a_fixed_number_and_decimals_can_be_set() {
    let mut offset = compute(Op::Offset, &["plant"]);
    offset.by = "-0.6".into();
    let v = values(&[("plant", "24.0")]);
    assert_eq!(offset.evaluate("T", &v).unwrap(), "23.4");
    offset.decimals = Some(0);
    assert_eq!(offset.evaluate("T", &v).unwrap(), "23");
}

#[test]
fn round_needs_decimals_and_rounds_half_away_from_zero() {
    let mut round = compute(Op::Round, &["a"]);
    round.decimals = Some(1);
    assert_eq!(
        round.evaluate("T", &values(&[("a", "2.25")])).unwrap(),
        "2.3"
    );
    assert_eq!(
        round.evaluate("T", &values(&[("a", "-2.25")])).unwrap(),
        "-2.3"
    );
    round.decimals = Some(0);
    assert_eq!(round.evaluate("T", &values(&[("a", "7.5")])).unwrap(), "8");
}

#[test]
fn copy_and_concat_work_on_text_and_skip_empty_parts() {
    let v = values(&[("a", "Pour"), ("b", ""), ("c", "12")]);
    assert_eq!(compute(Op::Copy, &["a"]).evaluate("T", &v).unwrap(), "Pour");
    let mut join = compute(Op::Concat, &["a", "b", "c"]);
    join.separator = " - ".into();
    assert_eq!(join.evaluate("T", &v).unwrap(), "Pour - 12");
    join.separator = String::new();
    assert_eq!(join.evaluate("T", &v).unwrap(), "Pour12");
}

#[test]
fn an_empty_input_gives_an_empty_number_and_a_bad_number_names_the_field() {
    let v = values(&[("a", ""), ("b", "2"), ("x", "abc")]);
    assert_eq!(compute(Op::Sum, &["a", "b"]).evaluate("T", &v).unwrap(), "");
    let error = compute(Op::Sum, &["x", "b"])
        .evaluate("Total", &v)
        .unwrap_err();
    assert!(error.contains("Total") && error.contains("x"), "{error}");
}

#[test]
fn the_shape_of_each_operation_is_checked() {
    let ok = |c: &Compute| c.check().is_ok();
    assert!(ok(&compute(Op::Sum, &["a"])));
    assert!(!ok(&compute(Op::Sum, &[])));
    assert!(ok(&compute(Op::Difference, &["a", "b"])));
    assert!(!ok(&compute(Op::Difference, &["a"])));
    assert!(!ok(&compute(Op::Difference, &["a", "b", "c"])));
    assert!(!ok(&compute(Op::Offset, &["a"])), "offset needs by");
    let mut offset = compute(Op::Offset, &["a"]);
    offset.by = "abc".into();
    assert!(!ok(&offset));
    offset.by = "-0,5".into();
    assert!(ok(&offset));
    assert!(!ok(&compute(Op::Round, &["a"])), "round needs decimals");
    let mut round = compute(Op::Round, &["a"]);
    round.decimals = Some(7);
    assert!(!ok(&round));
    round.decimals = Some(2);
    assert!(ok(&round));
    assert!(ok(&compute(Op::Copy, &["a"])));
    assert!(!ok(&compute(Op::Copy, &["a", "b"])));
    let many = (0..11).map(|n| format!("k{n}")).collect::<Vec<_>>();
    let many = many.iter().map(String::as_str).collect::<Vec<_>>();
    assert!(!ok(&compute(Op::Concat, &many)));
    let mut long = compute(Op::Concat, &["a"]);
    long.separator = "x".repeat(21);
    assert!(!ok(&long));
}
