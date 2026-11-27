//! The browser's own reveal pan (FR4), upstream `watchRevealPan` and
//! `panVisualViewportTo` (`keyboardHeight.ts`) and `watchPans` (`keyboardTarget.ts`).
//!
//! A browser that reveals a focused field itself pans the visual viewport to it after the
//! keyboard's report. GPUI's web build focuses its hidden input with `preventScroll` at
//! the canvas's top corner, where no keyboard covers it, and its canvas takes no touch
//! scroll, so the browser has no field to reveal and no document to scroll. The watch
//! still reads the visual viewport GPUI samples each frame, and puts back any pan that
//! comes while a field it owns has focus, as upstream does.

use gpui::{App, Window};

use super::height::{
    KEYBOARD_EPSILON, KeyboardField, focused_field, is_finger_on_screen, touch_starts,
};

/// Upstream `panVisualViewportTo`: pans the visual viewport so its top sits `top` px down
/// the layout viewport, without scrolling the document: a probe fixed there is scrolled
/// into view. On the web only; a native window's visual viewport is the window.
pub fn pan_visual_viewport_to(top: f64) {
    #[cfg(target_family = "wasm")]
    {
        use wasm_bindgen::JsCast as _;
        let Some(document) = web_sys::window().and_then(|w| w.document()) else {
            return;
        };
        let Ok(probe) = document.create_element("div") else {
            return;
        };
        let Ok(probe) = probe.dyn_into::<web_sys::HtmlElement>() else {
            return;
        };
        let style = probe.style();
        for (property, value) in [
            ("position", "fixed".to_string()),
            ("left", "0".to_string()),
            ("top", format!("{top}px")),
            ("width", "1px".to_string()),
            ("height", "1px".to_string()),
            ("visibility", "hidden".to_string()),
            ("pointer-events", "none".to_string()),
        ] {
            style.set_property(property, &value).ok();
        }
        let Some(parent) = document.body() else {
            return;
        };
        if parent.append_child(&probe).is_err() {
            return;
        }
        let options = web_sys::ScrollIntoViewOptions::new();
        options.set_block(web_sys::ScrollLogicalPosition::Start);
        options.set_inline(web_sys::ScrollLogicalPosition::Nearest);
        options.set_behavior(web_sys::ScrollBehavior::Instant);
        probe.scroll_into_view_with_scroll_into_view_options(&options);
        probe.remove();
    }
    #[cfg(not(target_family = "wasm"))]
    {
        let _ = top;
    }
}

/// Where the visual viewport sat at a report.
#[derive(Clone, Copy, Debug)]
struct AtReport {
    top: f64,
    starts: u64,
}

/// Upstream `watchRevealPan`: a watch for the browser's own reveal pan. The caller calls
/// [`RevealPanWatch::report`] on each report that changes the height; on a later report
/// [`RevealPanWatch::is_reveal_pan`] says the visual viewport has moved down with no
/// touch since, while a text field the caller owns has focus, and
/// [`RevealPanWatch::put_back`] pans it back.
pub struct RevealPanWatch {
    owns: Box<dyn Fn(&KeyboardField) -> bool>,
    at_report: Option<AtReport>,
    touched_at_report: bool,
}

fn viewport_top(window: &Window) -> f64 {
    f64::from(window.visual_viewport_bounds().origin.y)
}

impl RevealPanWatch {
    /// A watch for the fields `owns` accepts.
    pub fn new(owns: impl Fn(&KeyboardField) -> bool + 'static) -> Self {
        RevealPanWatch {
            owns: Box::new(owns),
            at_report: None,
            touched_at_report: false,
        }
    }

    /// A report changed the keyboard's height: remember where the viewport sat. A report
    /// while a finger is down leaves that drag's pan to the person.
    pub fn report(&mut self, window: &mut Window, cx: &mut App) {
        self.at_report = Some(AtReport {
            top: viewport_top(window),
            starts: touch_starts(window, cx),
        });
        self.touched_at_report = is_finger_on_screen(window, cx);
    }

    /// Whether the visual viewport has since moved down as a reveal pan.
    pub fn is_reveal_pan(&self, height: f64, window: &mut Window, cx: &mut App) -> bool {
        let Some(at) = self.at_report else {
            return false;
        };
        if self.touched_at_report || touch_starts(window, cx) != at.starts || height <= 0. {
            return false;
        }
        if viewport_top(window) <= at.top {
            return false;
        }
        focused_field(window, cx).is_some_and(|f| f.kind.text_entry && (self.owns)(&f))
    }

    /// Pans the visual viewport back to where it sat at the report.
    pub fn put_back(&self) {
        pan_visual_viewport_to(self.at_report.map_or(0., |a| a.top));
    }
}

/// Upstream `watchPans`: puts back a pan the browser makes on its own after a keyboard
/// report. Feed [`PanWatch::report`] each height the module reports. The caller's own pan
/// is the new rest once reported ([`PanWatch::expect_pan`]).
pub struct PanWatch {
    watch: RevealPanWatch,
    last_height: f64,
    own_pan_top: Option<f64>,
}

impl PanWatch {
    /// A watch over the fields `owns` accepts, from the height now.
    pub fn new(height: f64, owns: impl Fn(&KeyboardField) -> bool + 'static) -> Self {
        PanWatch {
            watch: RevealPanWatch::new(owns),
            last_height: height,
            own_pan_top: None,
        }
    }

    /// A report of `height`.
    pub fn report(&mut self, height: f64, window: &mut Window, cx: &mut App) {
        let top = viewport_top(window);
        if height != self.last_height {
            if self.own_pan_top.is_none()
                && self.last_height > KEYBOARD_EPSILON
                && self.watch.is_reveal_pan(height, window, cx)
            {
                self.watch.put_back();
            }
            self.last_height = height;
            self.watch.report(window, cx);
        } else if self
            .own_pan_top
            .is_some_and(|own| (top - own).abs() <= KEYBOARD_EPSILON)
        {
            self.own_pan_top = None;
            self.watch.report(window, cx);
        } else if self.watch.is_reveal_pan(height, window, cx) {
            self.watch.put_back();
        }
    }

    /// A pan the caller makes itself to `top`: its report is the new rest.
    pub fn expect_pan(&mut self, top: f64) {
        self.own_pan_top = Some(top);
    }
}
