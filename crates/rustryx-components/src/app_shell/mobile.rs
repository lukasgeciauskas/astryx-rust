//! Upstream `AppShell/AppShellMobileContext.tsx`: AppShell's narrow-screen state, read by
//! MobileNav, MobileNavToggle and every row in the mobile drawer. AppShell provides it;
//! a caller may provide one by hand with `rustryx_primitives::provide`.

use std::cell::Cell;
use std::rc::Rc;

use gpui::{App, Bounds, Pixels, Window};
use rustryx_primitives::use_context;

/// Opens (true) or closes (false) the drawer.
pub type OpenFn = Rc<dyn Fn(bool, &mut Window, &mut App)>;

/// Upstream `AppShellMobileContextValue`.
#[derive(Clone)]
pub struct AppShellMobile {
    /// Upstream `isMobile`: the viewport is below the mobile breakpoint.
    pub is_mobile: bool,
    /// Upstream `isMobileNavOpen`.
    pub is_open: bool,
    /// Upstream `isMobileNavEnabled`.
    pub enabled: bool,
    /// Upstream `hasAutoToggle`: auto-placed toggles render (off with a custom toggle).
    pub has_auto_toggle: bool,
    /// Opens or closes the drawer: upstream `openMobileNav` and `closeMobileNav`.
    pub set_open: OpenFn,
    /// Where the toggle is, so a drawer opened from the keyboard knows its opener's side.
    pub(crate) opener: Rc<Cell<Option<Bounds<Pixels>>>>,
}

impl Default for AppShellMobile {
    /// Upstream's default value: not mobile, closed, disabled, with auto toggles.
    fn default() -> Self {
        AppShellMobile {
            is_mobile: false,
            is_open: false,
            enabled: false,
            has_auto_toggle: true,
            set_open: Rc::new(|_, _, _| {}),
            opener: Rc::default(),
        }
    }
}

impl AppShellMobile {
    /// A mobile layout with its navigation enabled, open or closed.
    pub fn new(is_open: bool, set_open: impl Fn(bool, &mut Window, &mut App) + 'static) -> Self {
        AppShellMobile {
            is_mobile: true,
            is_open,
            enabled: true,
            set_open: Rc::new(set_open),
            ..Default::default()
        }
    }

    /// Upstream `openMobileNav`.
    pub fn open(&self, window: &mut Window, cx: &mut App) {
        (self.set_open)(true, window, cx);
    }

    /// Upstream `closeMobileNav`.
    pub fn close(&self, window: &mut Window, cx: &mut App) {
        (self.set_open)(false, window, cx);
    }

    /// Upstream `toggleMobileNav`.
    pub fn toggle(&self, window: &mut Window, cx: &mut App) {
        (self.set_open)(!self.is_open, window, cx);
    }
}

/// Upstream `useAppShellMobile`: the enclosing AppShell's state, else the default.
pub fn use_app_shell_mobile(cx: &App) -> AppShellMobile {
    use_context::<AppShellMobile>(cx).unwrap_or_default()
}
