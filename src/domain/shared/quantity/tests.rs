//! 权威数值类型的单元测试。

use super::{BasisPoints, PowerScore, RuleVersion};

#[test]
fn basis_points_rejects_negative_values() {
    assert_eq!(BasisPoints::new(0), Some(BasisPoints::ZERO));
    assert_eq!(BasisPoints::new(12_345), Some(BasisPoints::signed(12_345)));
    assert_eq!(BasisPoints::new(-1), None);
}

#[test]
fn hundred_percent_is_ten_thousand_basis_points() {
    assert_eq!(BasisPoints::HUNDRED_PERCENT.value(), 10_000);
    assert_eq!(BasisPoints::SCALE, 10_000);
}

#[test]
fn saturating_scale_matches_multiply_then_divide() {
    let half = BasisPoints::new(5_000).expect("half");
    assert_eq!(half.saturating_scale(300), 150);
    assert_eq!(half.saturating_scale(-300), -150);
    assert_eq!(half.saturating_scale(0), 0);
    assert_eq!(BasisPoints::HUNDRED_PERCENT.saturating_scale(77), 77);
    assert_eq!(BasisPoints::ZERO.saturating_scale(77), 0);
}

#[test]
fn saturating_scale_never_wraps_or_flips_sign() {
    let large = BasisPoints::signed(i64::MAX);
    assert_eq!(large.saturating_scale(i64::MAX), i64::MAX);
    let negative_large = BasisPoints::signed(i64::MIN);
    assert_eq!(negative_large.saturating_scale(i64::MAX), i64::MIN);
}

#[test]
fn rule_version_requires_positive_value() {
    assert_eq!(RuleVersion::new(1), Some(RuleVersion::INITIAL));
    assert_eq!(RuleVersion::new(42).map(RuleVersion::value), Some(42));
    assert_eq!(RuleVersion::new(0), None);
}

#[test]
fn power_score_saturates_negative_input() {
    assert_eq!(PowerScore::saturating_new(0).value(), 0);
    assert_eq!(PowerScore::saturating_new(12_345).value(), 12_345);
    assert_eq!(PowerScore::saturating_new(-1).value(), 0);
}
