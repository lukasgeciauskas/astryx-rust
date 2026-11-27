//! Upstream `InfoTip.test.tsx`: the lab entry is Core's component, and every export is
//! deprecated (component:InfoTip FR4).

use std::any::TypeId;

#[test]
#[allow(deprecated)]
fn is_the_core_component() {
    assert_eq!(
        TypeId::of::<super::InfoTip>(),
        TypeId::of::<rustryx_components::overlay::InfoTip>()
    );
    assert_eq!(
        TypeId::of::<super::InfoTipSize>(),
        TypeId::of::<rustryx_components::icon::IconSize>()
    );
}

#[test]
fn marks_each_of_its_exports_deprecated() {
    // Static metadata only: the compiler warns where they are used, and nothing warns
    // at run time. The attribute right above each export, whatever it says.
    let source = include_str!("mod.rs");
    for name in ["InfoTip", "InfoTipSize"] {
        let export = format!("pub type {name} =");
        let at = source
            .find(&export)
            .unwrap_or_else(|| panic!("{name} is exported"));
        let before = &source[..at];
        let attribute = before
            .rfind("#[deprecated")
            .expect("a deprecation attribute");
        let between = &before[attribute..];
        assert!(
            !between.contains("pub type") && !between.contains("pub const"),
            "{name}'s own attribute"
        );
    }
}
