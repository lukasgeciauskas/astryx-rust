//! The ephemeral arrow-key hint ("← → to navigate"), upstream
//! `useKeyboardHint` with the anchored hint layer it shares with reorder
//! mode (`hooks/anchoredHint.tsx`).
//!
//! A composite with one tab stop and arrows inside (TabList, Toolbar,
//! SegmentedControl) shows it below its focused item when keyboard focus
//! first enters it from outside. It stays until the first arrow press,
//! Escape (through the layer stack, without moving focus) or focus leaving
//! the composite, and does not show again for that instance.
//!
//! GPUI tells no element's bounds by its focus, so the hint anchors to the
//! focused control's focus ring (`crate::focus` records where rings are
//! drawn): the innermost one inside the composite.

use std::time::Duration;

use astryx_i18n::{current as i18n, use_direction};
use astryx_primitives::{
    InteractionModality, LayerAlign, LayerOptions, Placement, anchored_layer, interaction_modality,
    position_layer,
};
use astryx_theme::{ThemeStyled, current};
use gpui::{
    AnyElement, App, Bounds, Context, ElementId, Entity, InteractiveElement, IntoElement,
    KeyDownEvent, ParentElement, Pixels, Stateful, Styled, Task, Window, div, relative,
};

use crate::content::Kbd;
use crate::doc::{ComponentDoc, PropMapping, RustApi};

const fn p(prop: &'static str, rust: RustApi) -> PropMapping {
    PropMapping { prop, rust }
}

pub const USE_KEYBOARD_HINT_DOC: ComponentDoc = ComponentDoc {
    name: "useKeyboardHint",
    rust_type: "astryx::components::keyboard_hint::use_keyboard_hint",
    props: &[
        p("options", RustApi::Argument("KeyboardHintOptions")),
        p(
            "options.orientation",
            RustApi::Method("KeyboardHintOptions::new"),
        ),
        p(
            "options.dismissAfterMs",
            RustApi::Method("KeyboardHintOptions::dismiss_after"),
        ),
        p(
            "options.isEnabled",
            RustApi::Method("KeyboardHintOptions::enabled"),
        ),
    ],
    example: include_str!("segmented/examples.rs"),
};

/// Which arrows the composite answers: the hint's glyphs.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum KeyboardHintOrientation {
    /// ← →
    #[default]
    Horizontal,
    /// ↑ ↓
    Vertical,
    /// ← → ↑ ↓
    Both,
}

impl KeyboardHintOrientation {
    fn keys(self) -> &'static [&'static str] {
        match self {
            Self::Horizontal => &["left", "right"],
            Self::Vertical => &["up", "down"],
            Self::Both => &["left", "right", "up", "down"],
        }
    }
}

/// Upstream `UseKeyboardHintOptions`.
#[derive(Clone, Debug)]
pub struct KeyboardHintOptions {
    pub orientation: KeyboardHintOrientation,
    /// A time limit; by default none, so the hint stays until it is used
    /// (WCAG 1.4.13).
    pub dismiss_after: Option<Duration>,
    pub enabled: bool,
}

impl Default for KeyboardHintOptions {
    fn default() -> Self {
        Self {
            orientation: KeyboardHintOrientation::Horizontal,
            dismiss_after: None,
            enabled: true,
        }
    }
}

impl KeyboardHintOptions {
    pub fn new(orientation: KeyboardHintOrientation) -> Self {
        Self {
            orientation,
            ..Self::default()
        }
    }

    pub fn dismiss_after(mut self, after: Duration) -> Self {
        self.dismiss_after = Some(after);
        self
    }

    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }
}

#[derive(Default)]
struct HintState {
    showing: bool,
    dismissed: bool,
    was_within: bool,
    /// The last focus ring read.
    seen: u64,
    timer: Option<Task<()>>,
}

/// One composite's hint, for this frame (upstream `UseKeyboardHintReturn`).
#[derive(Clone)]
pub struct KeyboardHint {
    id: ElementId,
    state: Entity<HintState>,
    orientation: KeyboardHintOrientation,
    /// The controls whose rings were drawn last frame.
    rings: Vec<Bounds<Pixels>>,
}

/// The hint of the composite `id`. `within` says whether focus is inside
/// the composite now: entering it by keyboard shows the hint, leaving it
/// dismisses it.
pub fn use_keyboard_hint(
    id: &ElementId,
    options: KeyboardHintOptions,
    within: bool,
    window: &mut Window,
    cx: &mut App,
) -> KeyboardHint {
    let state = window.use_keyed_state(
        ElementId::Name(format!("{id:?}-keyboard-hint").into()),
        cx,
        |_, _: &mut Context<HintState>| HintState::default(),
    );
    let keyboard = interaction_modality(window) == InteractionModality::Keyboard;
    let (entered, left, showing) = {
        let s = state.read(cx);
        (within && !s.was_within, !within && s.was_within, s.showing)
    };
    if entered && keyboard && options.enabled && !state.read(cx).dismissed {
        state.update(cx, |s, _| s.showing = true);
        if let Some(after) = options.dismiss_after {
            let weak = state.downgrade();
            let task = window.spawn(cx, async move |cx| {
                cx.background_executor().timer(after).await;
                if let Some(state) = weak.upgrade() {
                    cx.update(|window, cx| {
                        state.update(cx, |s, _| {
                            s.showing = false;
                            s.dismissed = true;
                        });
                        window.refresh();
                    })
                    .ok();
                }
            });
            state.update(cx, |s, _| s.timer = Some(task));
        }
        // The focused item's ring draws this frame: show at it on the next.
        window.defer(cx, |window, _| window.refresh());
    } else if left && showing {
        state.update(cx, |s, _| {
            s.showing = false;
            s.dismissed = true;
            s.timer = None;
        });
    }
    if !options.enabled && showing {
        state.update(cx, |s, _| s.showing = false);
    }
    let seen = state.read(cx).seen;
    let (latest, rings) = crate::focus::rings_since(seen, cx);
    state.update(cx, |s, _| {
        s.was_within = within;
        s.seen = latest;
    });
    KeyboardHint {
        id: id.clone(),
        state,
        orientation: options.orientation,
        rings,
    }
}

fn dismiss(state: &Entity<HintState>, window: &mut Window, cx: &mut App) {
    state.update(cx, |s, _| {
        s.showing = false;
        s.dismissed = true;
        s.timer = None;
    });
    window.refresh();
}

impl KeyboardHint {
    /// Whether the hint shows now.
    pub fn is_showing(&self, cx: &App) -> bool {
        self.state.read(cx).showing
    }

    /// The composite's container: the first arrow press dismisses the hint
    /// (the person found the keys), and while it shows the hint sits below
    /// the start of the focused item.
    pub fn wrap(
        &self,
        container: Stateful<gpui::Div>,
        window: &mut Window,
        cx: &mut App,
    ) -> AnyElement {
        let state = self.state.clone();
        let container = container.capture_key_down(move |event: &KeyDownEvent, window, cx| {
            if state.read(cx).showing
                && matches!(
                    event.keystroke.key.as_str(),
                    "left" | "right" | "up" | "down"
                )
            {
                dismiss(&state, window, cx);
            }
        });
        // Always in the layer (transparent without its surface), so the
        // container's element path, and the state its controls keep, holds
        // as the hint comes and goes.
        let layer = anchored_layer(
            ElementId::Name(format!("{:?}-keyboard-hint-layer", self.id).into()),
            container,
        );
        if !self.is_showing(cx) {
            return layer.into_any_element();
        }
        let theme = current(window, cx);
        let t = theme.tokens();
        let rem = window.rem_size();
        let selector = format!(
            "{}-keyboard-hint",
            match &self.id {
                ElementId::Name(n) => n.to_string(),
                other => format!("{other:?}"),
            }
        );
        let hint = div()
            .debug_selector(move || selector.clone())
            .flex()
            .items_center()
            .bg(t.color.background_popover)
            .rounded(t.radius.element)
            .shadow_token(&t.shadow.low)
            .py(t.spacing.s1)
            .px(t.spacing.s2)
            .text_size(t.text.supporting.size.to_pixels(rem))
            .line_height(relative(t.text.supporting.leading))
            .text_color(t.color.text_secondary)
            .whitespace_nowrap()
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(t.spacing.s1)
                    .children(self.orientation.keys().iter().map(|k| Kbd::new(*k))),
            )
            .child(
                div()
                    .ml(t.spacing.s1)
                    .child(i18n(cx).t("@astryx.keyboardHint.toNavigate")),
            );
        let rings = self.rings.clone();
        let dismiss_state = self.state.clone();
        layer
            .surface(
                hint,
                LayerOptions {
                    placement: Placement::Below,
                    align: LayerAlign::Start,
                    rtl: use_direction(cx).is_rtl(),
                    ..LayerOptions::default()
                },
                move |window, cx| dismiss(&dismiss_state, window, cx),
            )
            .light_dismiss(false)
            .position_with(move |container, size, viewport, options| {
                // The innermost focus ring inside the composite: its focused
                // item. Its start while none has drawn yet.
                let inside = |r: &Bounds<Pixels>| {
                    r.left() >= container.left() - gpui::px(1.)
                        && r.right() <= container.right() + gpui::px(1.)
                        && r.top() >= container.top() - gpui::px(1.)
                        && r.bottom() <= container.bottom() + gpui::px(1.)
                };
                let anchor = rings
                    .iter()
                    .filter(|r| inside(r))
                    .min_by(|a, b| {
                        let area = |r: &&Bounds<Pixels>| {
                            f32::from(r.size.width) * f32::from(r.size.height)
                        };
                        area(a).total_cmp(&area(b))
                    })
                    .copied()
                    .unwrap_or(container);
                position_layer(anchor, size, viewport, options)
            })
            .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use gpui::{
        Context, IntoElement, Modifiers, ParentElement, Render, Styled, TestAppContext,
        VisualTestContext, Window, div, px,
    };

    use crate::button::Button;
    use crate::tabs::{Tab, TabList};
    use crate::toolbar::Toolbar;

    struct Host;

    impl Render for Host {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            div()
                .p(px(20.))
                .flex()
                .flex_col()
                .gap(px(20.))
                .child(Button::new("before", "Before"))
                .child(
                    TabList::new("t", "a")
                        .tab(Tab::new("a", "Alpha"))
                        .tab(Tab::new("b", "Beta"))
                        .tab(Tab::new("c", "Gamma")),
                )
                .child(Button::new("after", "After"))
                .child(
                    Toolbar::new("tools", "Tools")
                        .orientation(crate::layout::Orientation::Vertical)
                        .start_content(
                            div()
                                .flex()
                                .flex_col()
                                .child(Button::new("x", "X"))
                                .child(Button::new("y", "Y")),
                        ),
                )
        }
    }

    fn host(cx: &mut TestAppContext) -> &mut VisualTestContext {
        cx.update(|cx| {
            astryx_theme::init(cx);
            astryx_primitives::focus::init(cx);
        });
        let (_, cx) = cx.add_window_view(|_, _| Host);
        cx.update(|window, _| window.activate_window());
        settle(cx);
        cx
    }

    fn settle(cx: &mut VisualTestContext) {
        for _ in 0..3 {
            cx.update(|window, _| window.refresh());
            cx.run_until_parked();
        }
    }

    fn press(cx: &mut VisualTestContext, keys: &str) {
        cx.simulate_keystrokes(keys);
        settle(cx);
    }

    fn shown(cx: &mut VisualTestContext, selector: &'static str) -> bool {
        cx.debug_bounds(selector).is_some()
    }

    #[gpui::test]
    fn shows_on_keyboard_entry_and_goes_with_the_first_arrow(cx: &mut TestAppContext) {
        let cx = host(cx);
        press(cx, "tab");
        assert!(!shown(cx, "t-keyboard-hint"));
        press(cx, "tab");
        assert!(
            shown(cx, "t-keyboard-hint"),
            "keyboard focus entered the tabs"
        );
        // Below the tabs, at the focused tab's start.
        let hint = cx.debug_bounds("t-keyboard-hint").unwrap();
        let before = cx.debug_bounds("before-hint-probe");
        assert!(before.is_none() || hint.top() > before.unwrap().bottom());
        press(cx, "right");
        assert!(!shown(cx, "t-keyboard-hint"));
        // Out and back in: it does not show again.
        press(cx, "tab");
        press(cx, "shift-tab");
        assert!(!shown(cx, "t-keyboard-hint"));
    }

    #[gpui::test]
    fn escape_dismisses_without_moving_focus_and_leaving_dismisses(cx: &mut TestAppContext) {
        let cx = host(cx);
        press(cx, "tab");
        press(cx, "tab");
        assert!(shown(cx, "t-keyboard-hint"));
        press(cx, "escape");
        assert!(!shown(cx, "t-keyboard-hint"));
        // Focus stayed on the tab: the arrows still move among the tabs.
        press(cx, "tab");
        press(cx, "tab");
        // The vertical toolbar's own hint shows the up and down keys.
        assert!(shown(cx, "tools-keyboard-hint"));
        press(cx, "shift-tab");
        assert!(!shown(cx, "tools-keyboard-hint"), "focus left the toolbar");
    }

    #[gpui::test]
    fn a_pointer_focus_shows_nothing(cx: &mut TestAppContext) {
        let cx = host(cx);
        // The second tab sits right of the first, at the strip's top.
        let after = cx.debug_bounds("t-keyboard-hint");
        assert!(after.is_none());
        cx.simulate_click(gpui::point(px(90.), px(80.)), Modifiers::none());
        settle(cx);
        assert!(!shown(cx, "t-keyboard-hint"));
    }
}
