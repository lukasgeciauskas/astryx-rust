//! `TabBar`: iOS 26's floating tab bar, the app's top-level sections as a capsule at
//! the bottom of the screen with a selection lens that slides between them (upstream lab
//! `TabBar` and `TabBarItem`).
//!
//! Layout is UIKit's measured geometry ([`model`]): a 62 px capsule, 21 px from the
//! screen's sides and bottom and no wider than 16 + 86 px per tab, with items spread
//! from 4 px inside its ends. Each tab's button spans to halfway to its neighbours, so a
//! press goes to the nearest item. Past five tabs the bar shows the first four and a
//! More tab, which `value` selects as `more_value` or as any tab past the fourth.
//!
//! Under a fine pointer a press is a plain button press and the lens slides on the
//! theme's medium duration and standard easing. Under a finger (a coarse primary
//! pointer) UIKit's motion plays: the lens travels to a landing finger and lifts, the
//! capsule flexes, a drag carries the lens (rubber-banded past the end items) and the
//! lift chooses the item nearest the finger, the lens settling onto it. While the lens
//! moves under a finger, a copy of the items in their selected look rides in it. Under
//! Reduce Motion a press scales nothing and a change from a key or the app cross-fades.
//!
//! The root stands for upstream's `position: fixed`: it is absolutely positioned
//! along the bottom of its nearest positioned ancestor (a window-filling app root puts
//! it at the bottom of the window).

pub(crate) mod engine;
pub mod examples;
pub mod model;

#[cfg(test)]
mod tests;

use std::collections::HashMap;
use std::rc::Rc;

use gpui::prelude::*;
use gpui::{
    AnyElement, App, Bounds, ElementId, Entity, Global, IntoElement, Pixels, Role, ScrollHandle,
    SharedString, StyleRefinement, TextAlign, WeakEntity, Window, canvas, div, px,
};
use rustryx_components::doc::{ComponentDoc, PropMapping, RustApi};
use rustryx_components::icon::{Icon, IconSize, IconSource};
use rustryx_i18n::{Arg, current as i18n, use_direction};
use rustryx_primitives::gestures::host::{
    GestureAttachment, after_gesture, gesture_area, gesture_host,
};
use rustryx_primitives::gestures::{
    GestureState, LongPressGestureRecognizer, LongPressOptions, PointerKind, RecognizerOptions,
};
use rustryx_primitives::motion::{AnimateOptions, MotionProperty, MotionValues, SpringTransition};
use rustryx_primitives::scroll_area::{ScrollAxis, measure};
use rustryx_primitives::{
    ListFocus, ListKeys, coarse_primary_pointer, is_focus_visible, prefers_reduced_motion,
};
use rustryx_theme::color::mix_srgb;
use rustryx_theme::{CubicBezier, ThemeStyled, current};
use web_time::Instant;

use engine::{FrameEngine, spring};
use model::*;

/// Draws a tab's icon: called each time the bar, the lens's copy or the More list draws
/// it.
pub type IconFn = Rc<dyn Fn(&mut Window, &mut App) -> AnyElement>;

/// Core's Icon at size `lg` in the current text colour, for a tab's `icon`.
pub fn tab_icon(source: impl Into<IconSource>) -> IconFn {
    let source: IconSource = source.into();
    Rc::new(move |_, _| {
        Icon::new(source.clone())
            .size(IconSize::Lg)
            .into_any_element()
    })
}

/// One tab of a TabBar (upstream `TabBarItem`). It draws nothing on its own: TabBar,
/// and TabBarMoreList for the tabs past the fourth, read it and draw the tab.
#[derive(Clone)]
pub struct TabBarItem {
    value: SharedString,
    label: SharedString,
    icon: IconFn,
    selected_icon: Option<IconFn>,
    badge: Option<SharedString>,
    badge_label: Option<SharedString>,
    panel_id: Option<SharedString>,
    disabled: bool,
}

impl TabBarItem {
    /// A tab identified by `value` in `value`, `on_change` and `on_reselect` (upstream
    /// `value`), named `label` under its icon (upstream `label`), with `icon` (upstream
    /// `icon`, Core's Icon at size lg; see [`TabBarItem::icon_element`] for any element).
    pub fn new(
        value: impl Into<SharedString>,
        label: impl Into<SharedString>,
        icon: impl Into<IconSource>,
    ) -> Self {
        TabBarItem {
            value: value.into(),
            label: label.into(),
            icon: tab_icon(icon),
            selected_icon: None,
            badge: None,
            badge_label: None,
            panel_id: None,
            disabled: false,
        }
    }

    /// Upstream `icon` as any element, drawn in the current text colour.
    pub fn icon_element(
        mut self,
        icon: impl Fn(&mut Window, &mut App) -> AnyElement + 'static,
    ) -> Self {
        self.icon = Rc::new(icon);
        self
    }

    /// Upstream `selectedIcon`: the icon while the tab is selected (a filled variant).
    /// Defaults to `icon`.
    pub fn selected_icon(mut self, icon: impl Into<IconSource>) -> Self {
        self.selected_icon = Some(tab_icon(icon));
        self
    }

    /// Upstream `selectedIcon` as any element.
    pub fn selected_icon_element(
        mut self,
        icon: impl Fn(&mut Window, &mut App) -> AnyElement + 'static,
    ) -> Self {
        self.selected_icon = Some(Rc::new(icon));
        self
    }

    /// Upstream `badge`: a count or a short string over the icon's top-trailing corner.
    /// An empty string is no badge.
    pub fn badge(mut self, badge: impl ToString) -> Self {
        self.badge = Some(badge.to_string().into());
        self
    }

    /// Upstream `badgeLabel`: what assistive technology reads for the badge after the
    /// label ("3 unread"). Defaults to the badge itself.
    pub fn badge_label(mut self, label: impl Into<SharedString>) -> Self {
        self.badge_label = Some(label.into());
        self
    }

    /// Upstream `panelId`: the panel this tab controls (`aria-controls`; give the panel
    /// the same `aria_id`, as [`TabBarPanel`] does). Choosing the selected tab again
    /// scrolls that panel to its top.
    pub fn panel_id(mut self, panel_id: impl Into<SharedString>) -> Self {
        self.panel_id = Some(panel_id.into());
        self
    }

    /// Upstream `aria-disabled`: the tab stays focusable, dimmed, and is never chosen; a
    /// tap, a click, a key or a lift over it selects nothing, and the arrow keys skip it.
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    /// The tab's value.
    pub fn value(&self) -> &SharedString {
        &self.value
    }
}

/// An item's label, for TabBarMoreList.
pub(crate) fn item_label(item: &TabBarItem) -> &SharedString {
    &item.label
}

/// An item's badge, for TabBarMoreList.
pub(crate) fn item_badge(item: &TabBarItem) -> Option<&SharedString> {
    item.badge.as_ref()
}

/// Whether an item is disabled, for TabBarMoreList (upstream `isDisabledItem`).
pub(crate) fn item_disabled(item: &TabBarItem) -> bool {
    item.disabled
}

/// An item's icon, drawn, for TabBarMoreList.
pub(crate) fn item_icon(item: &TabBarItem, window: &mut Window, cx: &mut App) -> AnyElement {
    (item.icon)(window, cx)
}

/// Whether a badge shows (upstream `hasBadge`): set and not empty.
pub fn has_badge(badge: Option<&SharedString>) -> bool {
    badge.is_some_and(|b| !b.is_empty())
}

/// The tabs past the fourth when there are more than five, the ones a More tab holds
/// (upstream `overflowItems`).
pub fn overflow_items(items: &[TabBarItem]) -> &[TabBarItem] {
    if items.len() > MAX_TABS {
        &items[MAX_TABS - 1..]
    } else {
        &[]
    }
}

/// A badged item's accessible name (upstream `badgedName`): one catalog message with
/// its label and badge, so a tab and its More row read the same. None without a badge.
pub fn badged_name(item: &TabBarItem, cx: &App) -> Option<SharedString> {
    let badge = item.badge.as_ref().filter(|b| !b.is_empty())?;
    let spoken = item.badge_label.clone().unwrap_or_else(|| badge.clone());
    Some(i18n(cx).format(
        "@astryx.tabBar.badgedItem",
        &[
            ("label", Arg::from(item.label.as_ref())),
            ("badge", Arg::from(spoken.as_ref())),
        ],
    ))
}

// ---------------------------------------------------------------------------
// Panels

/// The panels' scroll areas this frame, by panel id: what a reselect scrolls.
#[derive(Default)]
struct Panels(HashMap<SharedString, Vec<ScrollHandle>>);

impl Global for Panels {}

/// A tab's panel (the element its `panel_id` names, upstream the element with that
/// `id`): it carries the id as its `aria_id`, so the tab's `aria-controls` reaches it,
/// and names its scroll areas, the panel's own first, in order. Choosing the selected
/// tab again scrolls the first that scrolls vertically to its top.
#[derive(IntoElement)]
pub struct TabBarPanel {
    id: SharedString,
    scroll: Vec<ScrollHandle>,
    style: StyleRefinement,
    children: Vec<AnyElement>,
}

impl TabBarPanel {
    /// The panel `panel_id` names.
    pub fn new(panel_id: impl Into<SharedString>) -> Self {
        TabBarPanel {
            id: panel_id.into(),
            scroll: Vec::new(),
            style: StyleRefinement::default(),
            children: Vec::new(),
        }
    }

    /// A scroll area of the panel (the handle its scroller tracks), in document order.
    pub fn scroll_area(mut self, handle: &ScrollHandle) -> Self {
        self.scroll.push(handle.clone());
        self
    }
}

impl Styled for TabBarPanel {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl ParentElement for TabBarPanel {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(elements);
    }
}

impl RenderOnce for TabBarPanel {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        cx.default_global::<Panels>()
            .0
            .insert(self.id.clone(), self.scroll);
        let mut el = div();
        el.style().refine(&self.style);
        el.id(ElementId::Name(self.id.clone()))
            .aria_id(self.id)
            .children(self.children)
    }
}

/// The tab's own scroll area (upstream `panelScrollArea`): the first of the panel's
/// that scrolls vertically, by Core's scroll test (more than a pixel of overflow).
fn panel_scroll_area(panel_id: &SharedString, cx: &App) -> Option<ScrollHandle> {
    let panels = cx.try_global::<Panels>()?;
    panels
        .0
        .get(panel_id)?
        .iter()
        .find(|h| measure(ScrollAxis::Block, h).block.is_scrollable)
        .cloned()
}

/// Brings the scroll area of `panel_id`'s panel back to its top, as iOS does when the
/// selected tab is chosen again (upstream `scrollPanelToTop`). No other scroller moves.
/// It glides there; under Reduce Motion it jumps.
pub fn scroll_panel_to_top(panel_id: Option<&SharedString>, window: &mut Window, cx: &mut App) {
    let Some(area) = panel_id.and_then(|id| panel_scroll_area(id, cx)) else {
        return;
    };
    if rustryx_primitives::keyboard::scroll_top_of(&area) == 0. {
        return;
    }
    if prefers_reduced_motion(cx) {
        rustryx_primitives::keyboard::scroll_top_to(&area, 0.);
        window.refresh();
    } else {
        rustryx_primitives::keyboard::smooth_scroll(&area, 0., window, cx).detach();
    }
}

// ---------------------------------------------------------------------------
// TabBar

type ValueFn = Rc<dyn Fn(SharedString, &mut Window, &mut App)>;

/// iOS 26's floating tab bar (upstream lab `TabBar`).
///
/// ```ignore
/// TabBar::new("sections", tab)
///     .on_change(|value, _, _| { /* show it */ })
///     .label("Sections")
///     .item(TabBarItem::new("home", "Home", "home"))
/// ```
#[derive(IntoElement)]
pub struct TabBar {
    id: ElementId,
    value: SharedString,
    on_change: Option<ValueFn>,
    on_reselect: Option<ValueFn>,
    label: Option<SharedString>,
    aria_label: Option<SharedString>,
    more_value: SharedString,
    items: Vec<TabBarItem>,
    style: StyleRefinement,
}

impl TabBar {
    /// A bar with the tab `value` selected (upstream `value`).
    pub fn new(id: impl Into<ElementId>, value: impl Into<SharedString>) -> Self {
        TabBar {
            id: id.into(),
            value: value.into(),
            on_change: None,
            on_reselect: None,
            label: None,
            aria_label: None,
            more_value: "more".into(),
            items: Vec::new(),
            style: StyleRefinement::default(),
        }
    }

    /// Upstream `onChange`: a tab's value when a different tab is chosen (a finger lifted
    /// over it, a click, or Enter or Space on a focused tab).
    pub fn on_change(mut self, f: impl Fn(SharedString, &mut Window, &mut App) + 'static) -> Self {
        self.on_change = Some(Rc::new(f));
        self
    }

    /// Upstream `onReselect`: the selected tab's value when it is chosen again, to pop its
    /// navigation stack to the root, as iOS does. Its panel also scrolls to its top.
    pub fn on_reselect(
        mut self,
        f: impl Fn(SharedString, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_reselect = Some(Rc::new(f));
        self
    }

    /// Upstream `label`: names the tab bar ("Tabs" in the reader's language without it).
    pub fn label(mut self, label: impl Into<SharedString>) -> Self {
        self.label = Some(label.into());
        self
    }

    /// Upstream `aria-label`: names the tab list, and wins over `label`.
    pub fn aria_label(mut self, label: impl Into<SharedString>) -> Self {
        self.aria_label = Some(label.into());
        self
    }

    /// Upstream `moreValue` (`"more"` by default): the More tab's value past five tabs.
    pub fn more_value(mut self, value: impl Into<SharedString>) -> Self {
        self.more_value = value.into();
        self
    }

    /// One of upstream's `children`: a tab, in order.
    pub fn item(mut self, item: TabBarItem) -> Self {
        self.items.push(item);
        self
    }

    /// Upstream `children`: the tabs, in order.
    pub fn items(mut self, items: impl IntoIterator<Item = TabBarItem>) -> Self {
        self.items.extend(items);
        self
    }
}

impl Styled for TabBar {
    /// Upstream `xstyle`: the root.
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

/// A tween of the lens's resting offset under a fine pointer (upstream's CSS
/// `translate` transition on the theme's medium duration and standard easing).
#[derive(Clone, Copy, Debug)]
struct Tween {
    from: f64,
    to: f64,
    start: Instant,
    ms: f64,
    ease: CubicBezier,
}

impl Tween {
    fn value(&self, now: Instant) -> (f64, bool) {
        if self.ms <= 0. {
            return (self.to, false);
        }
        let t = now.saturating_duration_since(self.start).as_secs_f64() * 1000. / self.ms;
        if t >= 1. {
            (self.to, false)
        } else {
            (
                self.from + (self.to - self.from) * f64::from(self.ease.ease(t as f32)),
                true,
            )
        }
    }
}

/// A finger or a pen on the bar (upstream `Press`).
#[derive(Clone, Debug)]
struct Press {
    centers: Vec<f64>,
    size: f64,
    left: f64,
    start_x: f64,
    last_x: f64,
    dragging: bool,
    has_lens: bool,
    ignored: bool,
    count: usize,
    paints: bool,
    painted: Option<usize>,
    /// The tab under the press now, where the lift would choose it.
    tab: Option<usize>,
}

/// The lens's layers (upstream `useTabBarLens`'s animators).
struct Lens {
    capsule: FrameEngine,
    body: FrameEngine,
    shadow: FrameEngine,
    glass: FrameEngine,
    magnify: FrameEngine,
}

/// What a bar keeps between frames.
struct BarState {
    /// The capsule's width and box, as last laid out.
    width: Option<f64>,
    bounds: Option<Bounds<Pixels>>,
    lens: Lens,
    press: Option<Press>,
    copy_shown: bool,
    seen_index: Option<usize>,
    /// The settle a lift started toward a newly chosen item.
    settle: Option<usize>,
    previous_index: Option<usize>,
    tween: Option<Tween>,
    /// A painted tab's fade after the lift (no tab selected).
    paint_fade: Option<(usize, Instant)>,
    latest: Latest,
}

/// The bar as its gestures read it, after each render.
#[derive(Clone, Default)]
struct Latest {
    count: usize,
    selected: Option<usize>,
    disabled: Vec<bool>,
    rtl: bool,
    touch: bool,
    choose: Option<ChooseFn>,
}

/// Chooses the tab at an index (upstream `choose`).
type ChooseFn = Rc<dyn Fn(usize, &mut Window, &mut App)>;

/// The press's recognizer: a long press with no delay, as UIKit's selection tracker.
struct Tracking {
    attachment: GestureAttachment,
    _press: LongPressGestureRecognizer,
}

fn values_x(x: f64) -> MotionValues {
    MotionValues::new().x(x)
}

impl BarState {
    fn chase_target(&self, x: f64, centers: &[f64]) -> f64 {
        let on_bar = self.press.as_ref().is_some_and(|p| p.tab.is_some());
        if on_bar {
            lens_target(x, centers)
        } else {
            centers[self
                .latest
                .selected
                .unwrap_or(0)
                .min(centers.len().saturating_sub(1))]
        }
    }

    /// Lifts (true) or lowers (false) a lens `size` px wide and flexes the capsule (upstream
    /// `lift`). Under Reduce Motion a press scales nothing: the glass and its shadow show
    /// at once, and fade only when lowered.
    fn lift(&self, lifted: bool, size: f64, touch_release: bool) {
        let options = AnimateOptions {
            touch_release,
            ..Default::default()
        };
        let lift = SpringTransition::new(spring(LIFT));
        let reduced = self.lens.body.reduced(touch_release);
        let target = if lifted { 1. } else { 0. };
        if reduced && lifted {
            self.lens.shadow.set(MotionValues::new().opacity(1.));
            self.lens.glass.set(MotionValues::new().opacity(1.));
        } else {
            self.lens
                .shadow
                .animate(MotionValues::new().opacity(target), lift, options);
            self.lens
                .glass
                .animate(MotionValues::new().opacity(target), lift, options);
        }
        if reduced {
            return;
        }
        let (sx, sy) = lift_scale(size);
        let scale = if lifted {
            MotionValues::new().scale_x(sx).scale_y(sy)
        } else {
            MotionValues::new().scale_x(1.).scale_y(1.)
        };
        self.lens.body.animate(scale, lift, options);
        self.lens.magnify.animate(
            MotionValues::new().scale(if lifted { LIFT_MAGNIFICATION } else { 1. }),
            lift,
            options,
        );
        self.lens.capsule.animate(
            MotionValues::new().scale(if lifted { FLEX_SCALE } else { 1. }),
            SpringTransition::new(spring(if lifted { FLEX_PRESS } else { FLEX_RELEASE })),
            options,
        );
    }
}

/// The tab under a press at window `x`, `y`: the nearest enabled item while the press
/// is on the capsule (each tab spans to halfway to its neighbours), else none. A
/// disabled tab is skipped, as off the bar.
fn tab_at(s: &BarState, x: f64, y: f64) -> Option<usize> {
    let bounds = s.bounds?;
    let (left, top) = (f64::from(bounds.origin.x), f64::from(bounds.origin.y));
    let (w, h) = (f64::from(bounds.size.width), f64::from(bounds.size.height));
    if x < left || x > left + w || y < top || y > top + h || s.latest.count == 0 {
        return None;
    }
    let centers = item_centers(w, s.latest.count, s.latest.rtl);
    let index = nearest_index(x - left, &centers);
    (!s.latest.disabled.get(index).copied().unwrap_or(false)).then_some(index)
}

fn begin(state: &Entity<BarState>, x: f64, y: f64, cx: &mut App) {
    state.update(cx, |s, cx| {
        let Some(bounds) = s.bounds else { return };
        let (count, selected) = (s.latest.count, s.latest.selected);
        let width = f64::from(bounds.size.width);
        let left = f64::from(bounds.origin.x);
        let centers = item_centers(width, count, s.latest.rtl);
        let size = item_width(width, count as f64);
        let fx = x - left;
        let tab = tab_at(s, x, y);
        // UIKit ignores a tap on a disabled tab: a finger that lands on one moves nothing
        // unless it then drags.
        let ignored = s
            .latest
            .disabled
            .get(nearest_index(fx, &centers))
            .copied()
            .unwrap_or(false);
        let paints = selected.is_none();
        s.press = Some(Press {
            centers: centers.clone(),
            size,
            left,
            start_x: fx,
            last_x: fx,
            dragging: false,
            has_lens: selected.is_some(),
            ignored,
            count,
            paints,
            painted: if paints { tab } else { None },
            tab,
        });
        cx.notify();
        let Some(selected) = selected else { return };
        if ignored {
            return;
        }
        // The lifted lens shows the selected look: its copy mounts now, under Reduce Motion
        // too. Under Reduce Motion the lens is put under the finger at once.
        s.copy_shown = true;
        let travel = s.chase_target(fx, &centers) - centers[selected];
        if s.lens.body.reduced(false) {
            s.lens.body.set(values_x(travel));
        } else {
            s.lens.body.animate(
                values_x(travel),
                SpringTransition::new(spring(LENS_TRAVEL)),
                AnimateOptions::default(),
            );
        }
        s.lift(true, size, false);
    });
}

fn track(state: &Entity<BarState>, x: f64, y: f64, cx: &mut App) {
    state.update(cx, |s, cx| {
        let tab = tab_at(s, x, y);
        let selected = s.latest.selected;
        let Some(press) = s.press.as_mut() else {
            return;
        };
        press.tab = tab;
        if press.paints {
            press.painted = tab;
        }
        let fx = x - press.left;
        press.last_x = fx;
        press.dragging |= (fx - press.start_x).abs()
            > rustryx_primitives::motion::uikit::gestures::LONG_PRESS_ALLOWABLE_MOVEMENT_PX;
        let (centers, size, dragging, has_lens) = (
            press.centers.clone(),
            press.size,
            press.dragging,
            press.has_lens,
        );
        if press.ignored {
            // The ignore ends once the finger drags or rests on an enabled tab.
            if !dragging && tab.is_none() {
                return;
            }
            press.ignored = false;
            if has_lens && selected.is_some() {
                s.copy_shown = true;
                s.lift(true, size, false);
            }
        }
        cx.notify();
        let Some(selected) = selected.filter(|_| has_lens) else {
            return;
        };
        let chase = s.chase_target(fx, &centers) - centers[selected];
        if s.lens.body.reduced(false) {
            s.lens.body.set(values_x(chase));
            return;
        }
        let travel = if dragging { LENS_TRACK } else { LENS_TRAVEL };
        s.lens.body.animate(
            values_x(chase),
            SpringTransition::new(spring(travel)),
            AnimateOptions::default(),
        );
    });
}

/// The press ended (upstream `end`): over a tab it chooses it, the lens settling onto it;
/// a cancel, or a lift off the bar, settles back onto the selected item.
fn end(state: &Entity<BarState>, cancelled: bool, window: &mut Window, cx: &mut App) {
    let deliver = state.update(cx, |s, cx| {
        let press = s.press.take()?;
        cx.notify();
        let now = cx.background_executor().now();
        if let Some(painted) = press.painted {
            s.paint_fade = Some((painted, now));
        }
        let selected = s.latest.selected;
        // The tab list changed under the press: its centres name the old list.
        let stale = press.count != s.latest.count;
        let chosen = if cancelled || stale { None } else { press.tab };
        let choose = s.latest.choose.clone();
        if press.ignored || !press.has_lens || selected.is_none() {
            // No lens moved: the tab the press lifted on still acts.
            return chosen.zip(choose);
        }
        let selected = selected?;
        let target = chosen.unwrap_or(selected);
        let settle = if stale {
            0.
        } else {
            press.centers[target] - press.centers[selected]
        };
        let release = AnimateOptions {
            touch_release: true,
            ..Default::default()
        };
        if s.lens.body.reduced(true) {
            s.lens.body.set(values_x(settle));
        } else {
            s.lens.body.animate(
                values_x(settle),
                SpringTransition::new(spring(LENS_SETTLE)),
                release,
            );
        }
        s.lift(false, press.size, true);
        if let Some(chosen) = chosen
            && chosen != selected
        {
            s.settle = Some(chosen);
        }
        chosen.zip(choose)
    });
    if let Some((index, choose)) = deliver {
        choose(index, window, cx);
        // The app kept the old selection: the lens settles back.
        state.update(cx, |s, _| {
            if let Some(settling) = s.settle
                && settling == index
                && s.latest.selected != Some(index)
            {
                s.settle = None;
                let release = AnimateOptions {
                    touch_release: true,
                    ..Default::default()
                };
                if s.lens.body.reduced(true) {
                    s.lens.body.set(values_x(0.));
                } else {
                    s.lens.body.animate(
                        values_x(0.),
                        SpringTransition::new(spring(LENS_SETTLE)),
                        release,
                    );
                }
            }
        });
    }
}

fn tracking(state: &Entity<BarState>, window: &mut Window, cx: &mut App) -> Tracking {
    let press = LongPressGestureRecognizer::new(LongPressOptions {
        base: RecognizerOptions {
            name: Some("TabBar.lens".into()),
            delays_touches_began: true,
            ..RecognizerOptions::default()
        },
        minimum_press_duration: 0.,
        allowable_movement: f64::INFINITY,
        cancels_on_extra_touch: true,
        ..LongPressOptions::default()
    });
    // A finger and a pen; a mouse press stays a plain button press.
    press.set_allowed_pointer_types(Some(vec![PointerKind::Touch, PointerKind::Pen]));
    let weak: WeakEntity<BarState> = state.downgrade();
    press.add_listener(move |r| {
        let (location, phase) = (r.location(), r.state());
        let weak = weak.clone();
        after_gesture(move |window, cx| {
            let Some(state) = weak.upgrade() else { return };
            if !state.read(cx).latest.touch {
                return;
            }
            match phase {
                GestureState::Began => begin(&state, location.x, location.y, cx),
                GestureState::Changed => track(&state, location.x, location.y, cx),
                GestureState::Ended => {
                    track(&state, location.x, location.y, cx);
                    end(&state, false, window, cx);
                }
                GestureState::Cancelled | GestureState::Failed => end(&state, true, window, cx),
                GestureState::Possible => {}
            }
        });
    });
    let attachment = gesture_host(window, cx).attach(&[press.any()]);
    Tracking {
        attachment,
        _press: press,
    }
}

/// A tab's column: where its button starts and how wide it is, and where its item starts
/// inside it (upstream `column`), px from the capsule's inline start.
fn column(index: usize, count: usize, width: f64) -> (f64, f64, f64) {
    let size = item_width(width, count as f64);
    if count <= 1 {
        return (0., width, width / 2. - size / 2.);
    }
    let pitch = (width - 2. * ITEM_INSET - size) / (count - 1) as f64;
    let half = pitch / 2.;
    let center = ITEM_INSET + size / 2. + pitch * index as f64;
    let edge = ITEM_INSET + size / 2. + half;
    if index == 0 {
        return (0., edge, ITEM_INSET);
    }
    let w = if index == count - 1 { edge } else { pitch };
    (center - half, w, half - size / 2.)
}

/// The lens's resting offset from the first item (upstream `lensOffset`).
fn lens_offset(index: usize, count: usize, width: f64) -> f64 {
    if count <= 1 {
        return 0.;
    }
    let size = item_width(width, count as f64);
    (width - 2. * ITEM_INSET - size) / (count - 1) as f64 * index as f64
}

/// An item's icon and label, in the plain or the selected look.
fn item_face(
    item: &TabBarItem,
    selected_look: bool,
    size: f64,
    t: &rustryx_theme::Tokens,
    window: &mut Window,
    cx: &mut App,
) -> Vec<AnyElement> {
    let icon = match (&item.selected_icon, selected_look) {
        (Some(selected), true) => selected(window, cx),
        _ => (item.icon)(window, cx),
    };
    vec![
        div()
            .absolute()
            .left(px(((size - ICON_SIZE) / 2.) as f32))
            .top(px((ICON_CENTER_Y - ICON_SIZE / 2.) as f32))
            .size(px(ICON_SIZE as f32))
            .flex()
            .items_center()
            .justify_center()
            .child(icon)
            .into_any_element(),
        div()
            .absolute()
            .left_0()
            .right_0()
            .top(px(LABEL_TOP as f32))
            .h(px((LABEL_SIZE * 1.2) as f32))
            .overflow_hidden()
            .text_align(TextAlign::Center)
            .whitespace_nowrap()
            .text_ellipsis()
            .text_size(px(LABEL_SIZE as f32))
            .line_height(px((LABEL_SIZE * 1.2) as f32))
            .font_weight(if selected_look {
                t.font.weight.semibold
            } else {
                t.font.weight.medium
            })
            .child(item.label.clone())
            .into_any_element(),
    ]
}

impl RenderOnce for TabBar {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = current(window, cx);
        let t = theme.tokens().clone();
        let rem = window.rem_size();
        let rtl = use_direction(cx).is_rtl();
        let touch = coarse_primary_pointer(window, cx);
        let i18n = i18n(cx);
        let now = cx.background_executor().now();
        // The tab list always has a name.
        let list_label = self
            .aria_label
            .clone()
            .or(self.label.clone())
            .unwrap_or_else(|| i18n.t("@astryx.tabList.label"));
        let overflow = overflow_items(&self.items).to_vec();
        let has_more = !overflow.is_empty();
        // UITabBarController shows four tabs and More past five.
        let items: Vec<TabBarItem> = if has_more {
            let mut shown = self.items[..MAX_TABS - 1].to_vec();
            shown.push(TabBarItem::new(
                self.more_value.clone(),
                i18n.t("@astryx.tabBar.more"),
                "moreHorizontal",
            ));
            shown
        } else {
            self.items.clone()
        };
        let count = items.len();
        // iOS keeps More selected while it shows a tab it holds.
        let more_selected = has_more
            && (self.value == self.more_value || overflow.iter().any(|i| i.value == self.value));
        let selected = if more_selected {
            Some(MAX_TABS - 1)
        } else {
            items.iter().position(|i| i.value == self.value)
        };
        let disabled: Vec<bool> = items.iter().map(|i| i.disabled).collect();

        // Choosing (upstream `choose`): another tab is a change; the selected one again, or
        // More while it holds a tab, is a reselect, and only the selected tab scrolls its
        // panel.
        let choose: ChooseFn = {
            let (items, value, more_value) =
                (items.clone(), self.value.clone(), self.more_value.clone());
            let (on_change, on_reselect) = (self.on_change.clone(), self.on_reselect.clone());
            Rc::new(move |index: usize, window: &mut Window, cx: &mut App| {
                let Some(item) = items.get(index) else { return };
                if item.disabled {
                    return;
                }
                if item.value == value || (item.value == more_value && more_selected) {
                    if item.value == value {
                        scroll_panel_to_top(item.panel_id.as_ref(), window, cx);
                    }
                    if let Some(f) = &on_reselect {
                        f(item.value.clone(), window, cx);
                    }
                } else if let Some(f) = &on_change {
                    f(item.value.clone(), window, cx);
                }
            })
        };

        let state = {
            let executor = cx.background_executor().clone();
            window.use_keyed_state(
                ElementId::Name(format!("{:?}-tab-bar", self.id).into()),
                cx,
                move |_, _| {
                    let engine = |initial| FrameEngine::new(executor.clone(), initial);
                    BarState {
                        width: None,
                        bounds: None,
                        lens: Lens {
                            capsule: engine(MotionValues::default()),
                            body: engine(MotionValues::default()),
                            shadow: engine(MotionValues::new().opacity(0.)),
                            glass: engine(MotionValues::new().opacity(0.)),
                            magnify: engine(MotionValues::default()),
                        },
                        press: None,
                        copy_shown: false,
                        seen_index: selected,
                        settle: None,
                        previous_index: selected,
                        tween: None,
                        paint_fade: None,
                        latest: Latest::default(),
                    }
                },
            )
        };
        let tracker = {
            let state = state.clone();
            window.use_keyed_state(
                ElementId::Name(format!("{:?}-tab-bar-press", self.id).into()),
                cx,
                move |window, cx| tracking(&state, window, cx),
            )
        };
        let attachment = tracker.read(cx).attachment.clone();
        attachment.set_rtl(rtl);

        let reduced = prefers_reduced_motion(cx);
        let width_now = state
            .read(cx)
            .width
            .unwrap_or_else(|| natural_width(count as f64));
        let frame = state.update(cx, |s, cx| {
            for e in [
                &s.lens.capsule,
                &s.lens.body,
                &s.lens.shadow,
                &s.lens.glass,
                &s.lens.magnify,
            ] {
                e.sync(window, cx);
            }
            s.latest = Latest {
                count,
                selected,
                disabled: disabled.clone(),
                rtl,
                touch,
                choose: Some(choose.clone()),
            };
            // The tab list changed under a held finger: the press ends as a cancel.
            if s.press.as_ref().is_some_and(|p| p.count != count) {
                s.press = None;
            }
            // A new selection under a finger is seen during render: the frame that starts
            // the slide already draws the copy.
            if s.seen_index != selected {
                if touch
                    && s.seen_index.is_some_and(|i| i < count)
                    && selected.is_some()
                    && !s.lens.body.reduced(false)
                {
                    s.copy_shown = true;
                }
                s.seen_index = selected;
            }
            // A new selection: the resting place jumps to the new item; the body starts
            // where the lens was on screen. After a lift it continues the settle the lift
            // started; otherwise it slides from rest.
            let from = std::mem::replace(&mut s.previous_index, selected);
            let from_lift = s.settle.take().is_some_and(|i| Some(i) == selected);
            if touch
                && let (Some(from), Some(to)) = (from, selected)
                && from != to
                && from < count
            {
                let centers = item_centers(width_now, count, rtl);
                let shift = centers[from] - centers[to];
                let offset = s.lens.body.get(MotionProperty::X, cx) + shift;
                let velocity = s.lens.body.velocity(MotionProperty::X, cx);
                s.lens.body.set(values_x(offset));
                let press = s.press.clone();
                if (press.is_some() || from_lift) && s.lens.body.reduced(false) {
                    let x =
                        press.map_or(0., |p| s.chase_target(p.last_x, &p.centers) - p.centers[to]);
                    s.lens.body.set(values_x(x));
                } else if let Some(p) = press {
                    // A finger is down: the lens keeps heading for it, now measured from its
                    // new resting place.
                    let x = s.chase_target(p.last_x, &p.centers) - p.centers[to];
                    let travel = if p.dragging { LENS_TRACK } else { LENS_TRAVEL };
                    s.lens.body.animate(
                        values_x(x),
                        SpringTransition::new(spring(travel)).velocity(values_x(velocity)),
                        AnimateOptions::default(),
                    );
                } else {
                    let travel = if from_lift { LENS_SETTLE } else { LENS_TRAVEL };
                    s.lens.body.animate(
                        values_x(0.),
                        SpringTransition::new(spring(travel)).velocity(values_x(velocity)),
                        AnimateOptions {
                            touch_release: from_lift,
                            ..Default::default()
                        },
                    );
                }
            }
            // Under a fine pointer the lens's resting place slides on the theme's medium
            // duration and standard easing; under Reduce Motion it jumps.
            if !touch
                && let (Some(from), Some(to)) = (from, selected)
                && from != to
            {
                let from_offset = s
                    .tween
                    .map_or(lens_offset(from, count, width_now), |tw| tw.value(now).0);
                let ms = if reduced {
                    0.
                } else {
                    t.duration.medium.as_secs_f64() * 1000.
                };
                s.tween = Some(Tween {
                    from: from_offset,
                    to: lens_offset(to, count, width_now),
                    start: now,
                    ms,
                    ease: t.ease.standard,
                });
            }
            let mut moving = false;
            for e in [
                &s.lens.capsule,
                &s.lens.body,
                &s.lens.shadow,
                &s.lens.glass,
                &s.lens.magnify,
            ] {
                moving |= e.drive(cx);
            }
            // The copy goes once everything the lens moves rests, with no finger down.
            if s.copy_shown && s.press.is_none() && !moving {
                s.copy_shown = false;
            }
            let rest = selected.map(|i| lens_offset(i, count, width_now));
            let mouse_offset = match (s.tween, rest) {
                (Some(tw), Some(rest)) if (tw.to - rest).abs() < 1e-6 => {
                    let (v, m) = tw.value(now);
                    moving |= m;
                    v
                }
                (_, Some(rest)) => rest,
                _ => 0.,
            };
            let painted = s.press.as_ref().and_then(|p| p.painted);
            let paint_fade = s.paint_fade.and_then(|(index, since)| {
                let ms = t.duration.exit.as_secs_f64() * 1000.;
                let ms = if reduced { ms.min(200.) } else { ms };
                let p = if ms <= 0. {
                    1.
                } else {
                    now.saturating_duration_since(since).as_secs_f64() * 1000. / ms
                };
                (p < 1.).then(|| {
                    moving = true;
                    (index, 1. - f64::from(t.ease.exit.ease(p as f32)))
                })
            });
            if paint_fade.is_none() {
                s.paint_fade = None;
            }
            (
                moving,
                s.copy_shown,
                mouse_offset,
                s.lens.body.get(MotionProperty::X, cx),
                s.lens.body.get(MotionProperty::ScaleX, cx),
                s.lens.body.get(MotionProperty::ScaleY, cx),
                s.lens.body.shown_opacity(cx),
                s.lens.shadow.get(MotionProperty::Opacity, cx),
                s.lens.glass.get(MotionProperty::Opacity, cx),
                s.lens.capsule.get(MotionProperty::ScaleX, cx),
                s.lens.magnify.get(MotionProperty::ScaleX, cx),
                painted,
                paint_fade,
            )
        });
        let (
            moving,
            copy_shown,
            mouse_offset,
            body_x,
            body_sx,
            body_sy,
            body_opacity,
            shadow,
            glass,
            flex,
            magnify,
            painted,
            paint_fade,
        ) = frame;
        if moving {
            window.request_animation_frame();
        }

        let width = width_now;
        let size = item_width(width, count as f64);
        let inline_x = |start: f64, w: f64| if rtl { width - start - w } else { start };
        let centers = item_centers(width, count, false);
        let list = ListFocus::new(&self.id, count, ListKeys::horizontal(), window, cx)
            .disabled(disabled.clone())
            .rtl(rtl);
        let tab_stop = list.tab_stop(Some(selected.unwrap_or(0)), window);

        // The tabs.
        let mut tabs = Vec::with_capacity(count);
        for (index, item) in items.iter().enumerate() {
            let is_selected = selected == Some(index);
            let (start, w, item_start) = column(index, count, width);
            let focus = list.item_handle(index, tab_stop);
            let selected_look = is_selected && !item.disabled && !copy_shown;
            let mut item_box = div()
                .absolute()
                .left(px(item_start as f32))
                .top(px(ITEM_INSET as f32))
                .w(px(size as f32))
                .h(px(ITEM_HEIGHT as f32))
                .rounded_full()
                .text_color(if item.disabled {
                    t.color.text_disabled
                } else if is_selected && !copy_shown {
                    t.color.accent
                } else {
                    t.color.text_primary
                })
                .children(item_face(item, selected_look, size, &t, window, cx));
            // With no tab selected, a held finger paints its tab at once; after the lift
            // only the paint's opacity fades, on the theme's exit duration and curve.
            let paint = if painted == Some(index) {
                Some(1.)
            } else {
                paint_fade.filter(|(i, _)| *i == index).map(|(_, a)| a)
            };
            if let Some(alpha) = paint {
                item_box = item_box.child(
                    div()
                        .absolute()
                        .inset_0()
                        .rounded_full()
                        .bg(t.color.overlay_pressed.opacity(alpha as f32)),
                );
            }
            if is_focus_visible(&focus, window) {
                item_box =
                    item_box.child(rustryx_components::focus::focus_ring(&t, px(9999.), rem));
            }
            let name = badged_name(item, cx).unwrap_or_else(|| item.label.clone());
            let choose = choose.clone();
            let selector = format!("{}-tab-{}", self.id, item.value);
            let mut tab = div()
                .id(ElementId::Name(format!("{:?}-tab-{index}", self.id).into()))
                .debug_selector(move || selector)
                .absolute()
                .top_0()
                .bottom_0()
                .left(px(inline_x(start, w) as f32))
                .w(px(w as f32))
                .track_focus(&focus)
                .role(Role::Tab)
                .aria_label(name)
                .aria_selected(is_selected)
                .aria_disabled(item.disabled)
                .child(item_box);
            theme.refine(
                tab.style(),
                "tab-bar-item",
                &[("selected", if is_selected { "selected" } else { "" })],
                None,
            );
            if let Some(panel) = &item.panel_id {
                tab = tab.aria_controls([panel.clone()]);
            }
            tab = if item.disabled {
                tab.cursor_default()
            } else {
                tab.cursor_pointer()
            };
            tabs.push(
                tab.on_click(move |_, window, cx| {
                    cx.stop_propagation();
                    choose(index, window, cx);
                })
                .into_any_element(),
            );
        }

        // The lens: at rest the pill under the selected item, which draws its own selected
        // look; while it moves under a finger it carries the copy over the items.
        let lens = selected.map(|index| {
            let rest = if touch {
                lens_offset(index, count, width)
            } else {
                mouse_offset
            };
            let clip_bg = mix_srgb(t.color.background_popover, t.color.text_primary, 0.09);
            let mut clip = div()
                .absolute()
                .inset_0()
                .overflow_hidden()
                .rounded_full()
                .bg(clip_bg)
                .child(
                    div()
                        .absolute()
                        .inset_0()
                        .rounded_full()
                        .bg(t.color.background_popover)
                        .border_1()
                        .border_color(t.color.text_primary.opacity(0.1))
                        .opacity(glass as f32),
                );
            if copy_shown {
                let (cx_, csx, csy) = copy_transform(body_x, body_sx, body_sy);
                let mut copy = div()
                    .absolute()
                    .left(px(-(ITEM_INSET + rest) as f32))
                    .top(px(-ITEM_INSET as f32))
                    .w(px(width as f32))
                    .h(px(BAR_HEIGHT as f32))
                    .translate_x(px(cx_ as f32))
                    .scale_xy(csx as f32, csy as f32);
                for (i, item) in items.iter().enumerate() {
                    let left = centers[i] - size / 2.;
                    copy = copy.child(
                        div()
                            .absolute()
                            .left(px(left as f32))
                            .top(px(ITEM_INSET as f32))
                            .w(px(size as f32))
                            .h(px(ITEM_HEIGHT as f32))
                            .scale(magnify as f32)
                            .text_color(if item.disabled {
                                t.color.text_disabled
                            } else {
                                t.color.accent
                            })
                            .children(item_face(item, !item.disabled, size, &t, window, cx)),
                    );
                }
                clip = clip.child(copy);
            }
            let body = div()
                .absolute()
                .inset_0()
                .translate_x(px(body_x as f32))
                .scale_xy(body_sx as f32, body_sy as f32)
                .child(
                    div()
                        .absolute()
                        .inset_0()
                        .rounded_full()
                        .shadow_token(&t.shadow.med)
                        .opacity(shadow as f32),
                )
                .child(clip);
            let left = ITEM_INSET + rest;
            div()
                .absolute()
                .left(px(inline_x(left, size) as f32))
                .top(px(ITEM_INSET as f32))
                .w(px(size as f32))
                .h(px(ITEM_HEIGHT as f32))
                .opacity(body_opacity as f32)
                .debug_selector(|| "tab-bar-lens".into())
                .child(body)
                .into_any_element()
        });

        // The badges: never animated, out of the accessibility tree.
        let badges = items
            .iter()
            .enumerate()
            .filter(|(_, i)| has_badge(i.badge.as_ref()))
            .map(|(index, item)| {
                let left = centers[index] + BADGE_OFFSET_X;
                let selector = format!("{}-badge-{}", self.id, item.value);
                div()
                    .debug_selector(move || selector)
                    .absolute()
                    .left(px(inline_x(left, 0.) as f32))
                    .top(px((ITEM_INSET + BADGE_OFFSET_Y) as f32))
                    .h(px(BADGE_HEIGHT as f32))
                    .min_w(px(BADGE_HEIGHT as f32))
                    .px(px(BADGE_PADDING_INLINE as f32))
                    .rounded(px((BADGE_HEIGHT / 2.) as f32))
                    .flex()
                    .items_center()
                    .justify_center()
                    .whitespace_nowrap()
                    .text_size(px(BADGE_FONT_SIZE as f32))
                    .line_height(px(BADGE_HEIGHT as f32))
                    .font_weight(t.font.weight.medium)
                    .bg(t.color.error)
                    .text_color(t.color.on_error)
                    .child(item.badge.clone().unwrap_or_default())
                    .into_any_element()
            })
            .collect::<Vec<_>>();

        let probe = state.downgrade();
        let measure_capsule = canvas(
            move |bounds, window, cx| {
                let _ = probe.update(cx, |s, _| {
                    let w = f64::from(bounds.size.width);
                    s.bounds = Some(bounds);
                    if s.width != Some(w) {
                        s.width = Some(w);
                        window.refresh();
                    }
                });
            },
            |_, _, _, _| {},
        )
        .absolute()
        .inset_0();

        #[cfg(test)]
        DRAWN.with(|d| {
            d.borrow_mut().insert(
                self.id.to_string(),
                Drawn {
                    list_label: list_label.clone(),
                    tabs: items
                        .iter()
                        .enumerate()
                        .map(|(i, item)| DrawnTab {
                            value: item.value.clone(),
                            name: badged_name(item, cx).unwrap_or_else(|| item.label.clone()),
                            selected: selected == Some(i),
                            disabled: item.disabled,
                            panel: item.panel_id.clone(),
                        })
                        .collect(),
                    tab_stop,
                    copy_shown,
                    lens: selected.is_some(),
                    badges: items
                        .iter()
                        .filter(|i| has_badge(i.badge.as_ref()))
                        .map(|i| (i.value.clone(), i.badge.clone().unwrap_or_default()))
                        .collect(),
                    lens_x: body_x,
                    flex,
                    touch,
                },
            );
        });
        let key_down = list.key_down_listener();
        let selector = format!("{}-capsule", self.id);
        let mut capsule = div()
            .id(ElementId::Name(format!("{:?}-capsule", self.id).into()))
            .debug_selector(move || selector)
            .relative()
            .flex_1()
            .min_w_0()
            .max_w(px(natural_width(count as f64) as f32))
            .h(px(BAR_HEIGHT as f32))
            .rounded_full()
            .bg(t.color.background_popover.opacity(0.78))
            .shadow_token(&t.shadow.med)
            .role(Role::TabList)
            .aria_label(list_label)
            .aria_orientation(gpui::Orientation::Horizontal)
            .on_key_down(key_down)
            .child(measure_capsule);
        if touch {
            capsule = capsule.scale(flex as f32);
        }
        // At rest the lens is under the items; while it carries the copy it is over them.
        if !copy_shown && let Some(lens) = lens {
            capsule = capsule.child(lens).children(tabs);
        } else {
            capsule = capsule.children(tabs).children(lens);
        }
        capsule = capsule.child(div().absolute().inset_0().children(badges));
        let mut root = div()
            .absolute()
            .left_0()
            .right_0()
            .bottom_0()
            .flex()
            .justify_center()
            .px(px(BAR_INSET as f32))
            .pb(px(BAR_INSET as f32));
        theme.refine(root.style(), "tab-bar", &[], None);
        root.style().refine(&self.style);
        root.child(gesture_area(&attachment, capsule))
    }
}

#[cfg(test)]
thread_local! {
    /// What each bar drew last, by its id, for tests.
    pub(crate) static DRAWN: std::cell::RefCell<HashMap<String, Drawn>> = std::cell::RefCell::default();
}

/// What a bar drew, for tests.
#[cfg(test)]
#[derive(Clone, Debug)]
pub(crate) struct Drawn {
    pub list_label: SharedString,
    pub tabs: Vec<DrawnTab>,
    pub tab_stop: Option<usize>,
    pub copy_shown: bool,
    pub lens: bool,
    pub badges: Vec<(SharedString, SharedString)>,
    pub lens_x: f64,
    pub flex: f64,
    pub touch: bool,
}

/// One tab as drawn, for tests.
#[cfg(test)]
#[derive(Clone, Debug)]
pub(crate) struct DrawnTab {
    pub value: SharedString,
    pub name: SharedString,
    pub selected: bool,
    pub disabled: bool,
    pub panel: Option<SharedString>,
}

const fn p(prop: &'static str, rust: RustApi) -> PropMapping {
    PropMapping { prop, rust }
}

/// Upstream lab `TabBar`.
pub const TAB_BAR_DOC: ComponentDoc = ComponentDoc {
    name: "TabBar",
    rust_type: "rustryx::lab::tab_bar::TabBar",
    props: &[
        p("value", RustApi::Argument("value")),
        p("onChange", RustApi::Method("on_change")),
        p("onReselect", RustApi::Method("on_reselect")),
        p("label", RustApi::Method("label")),
        p("children", RustApi::Method("item / items")),
        p("moreValue", RustApi::Method("more_value")),
    ],
    example: include_str!("examples.rs"),
};

/// Upstream lab `TabBarItem`.
pub const TAB_BAR_ITEM_DOC: ComponentDoc = ComponentDoc {
    name: "TabBarItem",
    rust_type: "rustryx::lab::tab_bar::TabBarItem",
    props: &[
        p("value", RustApi::Argument("value")),
        p("label", RustApi::Argument("label")),
        p("icon", RustApi::Argument("icon (or icon_element)")),
        p(
            "selectedIcon",
            RustApi::Method("selected_icon / selected_icon_element"),
        ),
        p("badge", RustApi::Method("badge")),
        p("badgeLabel", RustApi::Method("badge_label")),
        p("panelId", RustApi::Method("panel_id")),
        p("aria-disabled", RustApi::Method("disabled")),
    ],
    example: include_str!("examples.rs"),
};
