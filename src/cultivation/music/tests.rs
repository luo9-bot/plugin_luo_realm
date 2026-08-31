use crate::cultivation::CultivationSystem;
#[test]
fn module_exists() {
    assert!(!super::System.id().is_empty());
}
