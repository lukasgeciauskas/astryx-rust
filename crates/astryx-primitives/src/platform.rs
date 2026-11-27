//! The platform the app runs on, as upstream reads it (`utils/isApplePlatform.ts`,
//! `utils/primaryClick.ts`).
//!
//! Upstream asks the browser which platform it runs on, so a page in a Mac browser
//! behaves as a Mac app does. Here GPUI reports the platform at run time: natively the
//! one the app was built for, and on the web the platform the browser runs on, so the
//! same rules hold in both.

use gpui::{App, Modifiers, MouseButton, OperatingSystem};

/// Upstream `isApplePlatform`: macOS or iOS (natively, or a browser on one). There
/// `mod` in a shortcut is Cmd, shortcuts show Apple's modifier symbols, and a
/// control-click is the context click.
pub fn is_apple_platform(cx: &App) -> bool {
    matches!(
        cx.platform_info().os,
        OperatingSystem::Mac | OperatingSystem::Ios
    )
}

/// Upstream `isPrimaryClick` (AST-094 FR43): the main button, and not a control-click
/// on an Apple platform, which is that platform's context click. A right click and a
/// macOS control-click start no press.
pub fn is_primary_click(button: MouseButton, modifiers: &Modifiers, cx: &App) -> bool {
    button == MouseButton::Left && !(modifiers.control && is_apple_platform(cx))
}

/// Whether a press is the platform's context click: a right click, or a control-click
/// on an Apple platform.
pub fn is_context_click(button: MouseButton, modifiers: &Modifiers, cx: &App) -> bool {
    button == MouseButton::Right
        || (button == MouseButton::Left && modifiers.control && is_apple_platform(cx))
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{PlatformInfo, PlatformTarget, TestAppContext};

    fn on(os: OperatingSystem, cx: &mut TestAppContext) {
        cx.set_platform_info(PlatformInfo {
            os,
            target: PlatformTarget::Native,
        });
    }

    #[gpui::test]
    fn apple_platforms_are_macos_and_ios(cx: &mut TestAppContext) {
        for (os, apple) in [
            (OperatingSystem::Mac, true),
            (OperatingSystem::Ios, true),
            (OperatingSystem::Windows, false),
            (OperatingSystem::Linux, false),
        ] {
            on(os, cx);
            assert_eq!(cx.read(is_apple_platform), apple, "{os:?}");
        }
    }

    #[gpui::test]
    fn a_control_click_is_the_context_click_on_a_mac(cx: &mut TestAppContext) {
        let control = Modifiers {
            control: true,
            ..Modifiers::default()
        };
        let none = Modifiers::default();
        on(OperatingSystem::Mac, cx);
        cx.read(|cx| {
            assert!(!is_primary_click(MouseButton::Left, &control, cx));
            assert!(is_context_click(MouseButton::Left, &control, cx));
            assert!(is_primary_click(MouseButton::Left, &none, cx));
            assert!(!is_primary_click(MouseButton::Right, &none, cx));
            assert!(is_context_click(MouseButton::Right, &none, cx));
        });
        on(OperatingSystem::Windows, cx);
        cx.read(|cx| {
            assert!(is_primary_click(MouseButton::Left, &control, cx));
            assert!(!is_context_click(MouseButton::Left, &control, cx));
            assert!(is_context_click(MouseButton::Right, &none, cx));
        });
    }
}
