use super::*;

#[test]
fn default_is_tier1() {
    assert_eq!(default_tier(), IdracTier::Tier1);
    assert!(thermal_ok_stub());
}
