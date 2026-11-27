//! `NavigationStack`: drill-down navigation for phone layouts, UINavigationController's
//! counterpart (upstream lab `NavigationStack`, `NavigationScreen` and
//! `useNavigationStack`).
//!
//! The app owns the path: the entries pushed on the root screen, which `on_path_change`
//! updates. The stack renders the root screen and one screen per entry, the top one last,
//! keeps covered screens and their scroll positions alive, and makes them inert; screens
//! beyond the one underneath the top are kept but not drawn. The bar shows the top
//! screen's title, its end content and a round Back button with a chevron, named by the
//! screen it returns to.
//!
//! Pushes, pops and the back swipe move like iOS ([`transition`]): the new screen slides
//! in from the trailing edge over the old one, which slides 0.3 of the width under a 10 %
//! black dimming, on UIKit's springs, and a released swipe finishes or springs back by
//! how far and how fast it went. A push never waits for its animation. Under Reduce
//! Motion the screens cross-fade. A push moves focus to the new screen's title unless
//! focus is already in that screen; a pop returns it to the control that pushed.
//!
//! Under a finger the bar paints no background and no hairline, as iOS 26's does; once a
//! screen's content has moved 10 px under the bar, a veil of the body colour fades in at
//! the top of that screen. With a mouse the bar keeps its surface and hairline.

pub mod examples;
pub mod transition;

#[cfg(test)]
mod tests;

use std::collections::HashMap;
use std::rc::Rc;

use gpui::prelude::*;
use gpui::{
    AnyElement, App, Bounds, ElementId, Entity, FocusHandle, Hsla, IntoElement, Pixels, Role,
    ScrollHandle, SharedString, WeakEntity, Window, canvas, div, linear_color_stop,
    linear_gradient, px,
};
use rustryx_components::doc::{ComponentDoc, PropMapping, RustApi};
use rustryx_components::icon::{Icon, IconColor};
use rustryx_i18n::{current as i18n, use_direction};
use rustryx_primitives::gestures::host::{
    GestureAttachment, after_gesture, gesture_area, gesture_host,
};
use rustryx_primitives::gestures::{GestureState, PanDirection, PanGestureRecognizer, PanOptions};
use rustryx_primitives::motion::uikit::springs::{
    NAVIGATION_BAR_ELEMENTS, NAVIGATION_INTERACTIVE_POP_FINISH, NAVIGATION_POP, NAVIGATION_PUSH,
};
use rustryx_primitives::motion::{
    AnimateOptions, CrossfadeAs, MotionProperty, MotionValues, SpringDescription, SpringTransition,
};
use rustryx_primitives::{
    coarse_primary_pointer, control_focus, is_focus_visible, provide, use_context,
};
use rustryx_theme::current;

use crate::tab_bar::engine::FrameEngine;
use transition::*;

/// How a screen's title shows (upstream `titleDisplayMode`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TitleDisplayMode {
    /// In the bar (the default).
    #[default]
    Inline,
    /// Above the content in a large bold type, scrolling with it; once it has scrolled
    /// under the bar, the bar shows the title instead.
    Large,
}

/// One screen of a NavigationStack (upstream `NavigationScreen`): its title, the title
/// Back shows on the screen pushed after it, its end content and its content. It draws
/// nothing on its own; the stack reads it.
pub struct NavigationScreen {
    title: SharedString,
    back_title: Option<SharedString>,
    title_display_mode: TitleDisplayMode,
    end_content: Option<AnyElement>,
    children: Vec<AnyElement>,
}

impl NavigationScreen {
    /// A screen titled `title` (upstream `title`): shown in the bar while it is on top,
    /// and read first when it arrives.
    pub fn new(title: impl Into<SharedString>) -> Self {
        NavigationScreen {
            title: title.into(),
            back_title: None,
            title_display_mode: TitleDisplayMode::Inline,
            end_content: None,
            children: Vec::new(),
        }
    }

    /// Upstream `backTitle`: what Back reads on the screen pushed on top of this one.
    /// Defaults to the title.
    pub fn back_title(mut self, title: impl Into<SharedString>) -> Self {
        self.back_title = Some(title.into());
        self
    }

    /// Upstream `titleDisplayMode` (`Inline` by default).
    pub fn title_display_mode(mut self, mode: TitleDisplayMode) -> Self {
        self.title_display_mode = mode;
        self
    }

    /// Upstream `endContent`: content at the end of the bar while the screen is on top.
    pub fn end_content(mut self, content: impl IntoElement) -> Self {
        self.end_content = Some(content.into_any_element());
        self
    }
}

impl ParentElement for NavigationScreen {
    /// Upstream `children`: the screen's content. It scrolls on its own and keeps its
    /// scroll position while covered.
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(elements);
    }
}

type PathFn<T> = Rc<dyn Fn(Vec<T>, &mut Window, &mut App)>;
type ScreenFn<T> = Rc<dyn Fn(&T, usize, &mut Window, &mut App) -> NavigationScreen>;
type KeyFn<T> = Rc<dyn Fn(&T, usize) -> SharedString>;

/// What a screen inside the stack uses to push and pop (upstream
/// `NavigationStackApi`, from `useNavigationStack`).
pub struct NavigationStackApi<T> {
    push: PushFn<T>,
    pop: ActionFn,
    pop_to_root: ActionFn,
    depth: usize,
}

type PushFn<T> = Rc<dyn Fn(T, Option<FocusHandle>, &mut Window, &mut App)>;
type ActionFn = Rc<dyn Fn(&mut Window, &mut App)>;
/// A pop whose focus waits: the popped key, the revealed key, the control that pushed,
/// and the popped screen's card and title focus.
type PendingFocus = (
    SharedString,
    SharedString,
    Option<FocusHandle>,
    Option<(FocusHandle, FocusHandle)>,
);

impl<T> Clone for NavigationStackApi<T> {
    fn clone(&self) -> Self {
        NavigationStackApi {
            push: self.push.clone(),
            pop: self.pop.clone(),
            pop_to_root: self.pop_to_root.clone(),
            depth: self.depth,
        }
    }
}

impl<T> NavigationStackApi<T> {
    /// Upstream `push`: pushes an entry. The control that has focus now gets it back when
    /// the entry's screen pops.
    pub fn push(&self, entry: T, window: &mut Window, cx: &mut App) {
        let source = window.focused(cx);
        (self.push)(entry, source, window, cx);
    }

    /// Upstream `push(entry, {source})`: pushes an entry; `source` gets focus back when its
    /// screen pops.
    pub fn push_from(
        &self,
        entry: T,
        source: Option<FocusHandle>,
        window: &mut Window,
        cx: &mut App,
    ) {
        (self.push)(entry, source, window, cx);
    }

    /// Upstream `pop`: pops the top screen; nothing at the root.
    pub fn pop(&self, window: &mut Window, cx: &mut App) {
        (self.pop)(window, cx);
    }

    /// Upstream `popToRoot`: pops back to the root.
    pub fn pop_to_root(&self, window: &mut Window, cx: &mut App) {
        (self.pop_to_root)(window, cx);
    }

    /// Upstream `depth`: how many screens are pushed on the root.
    pub fn depth(&self) -> usize {
        self.depth
    }
}

/// The enclosing NavigationStack's push, pop and pop-to-root (upstream
/// `useNavigationStack`). Read it while rendering a screen's content.
///
/// # Panics
///
/// Outside a NavigationStack whose entries are `T`, as upstream throws.
pub fn use_navigation_stack<T: 'static>(cx: &App) -> NavigationStackApi<T> {
    use_context::<NavigationStackApi<T>>(cx)
        .expect("useNavigationStack must be used inside a NavigationStack")
}

/// The navigator (upstream lab `NavigationStack`).
///
/// ```ignore
/// NavigationStack::new("settings", NavigationScreen::new("Settings").child(rows), path)
///     .render_screen(|entry, _, _, _| NavigationScreen::new(entry.title.clone()))
///     .on_path_change(|path, _, cx| { /* store it */ })
/// ```
#[derive(IntoElement)]
pub struct NavigationStack<T: Clone + 'static> {
    id: ElementId,
    root: NavigationScreen,
    path: Vec<T>,
    on_path_change: Option<PathFn<T>>,
    render_screen: Option<ScreenFn<T>>,
    get_key: Option<KeyFn<T>>,
    interactive_pop: bool,
    label: Option<SharedString>,
}

impl<T: Clone + 'static> NavigationStack<T> {
    /// A stack over `root` (upstream `root`, which has no Back button) with `path` pushed
    /// on it, bottom first (upstream `path`): adding an entry at the end pushes its
    /// screen; removing the last pops it; removing several pops straight back.
    pub fn new(
        id: impl Into<ElementId>,
        root: NavigationScreen,
        path: impl IntoIterator<Item = T>,
    ) -> Self {
        NavigationStack {
            id: id.into(),
            root,
            path: path.into_iter().collect(),
            on_path_change: None,
            render_screen: None,
            get_key: None,
            interactive_pop: true,
            label: None,
        }
    }

    /// Upstream `onPathChange`: the new path when a screen pushes or pops, Back is pressed,
    /// or a back swipe is released past the point of no return.
    pub fn on_path_change(mut self, f: impl Fn(Vec<T>, &mut Window, &mut App) + 'static) -> Self {
        self.on_path_change = Some(Rc::new(f));
        self
    }

    /// Upstream `renderScreen`: the screen for an entry.
    pub fn render_screen(
        mut self,
        f: impl Fn(&T, usize, &mut Window, &mut App) -> NavigationScreen + 'static,
    ) -> Self {
        self.render_screen = Some(Rc::new(f));
        self
    }

    /// Upstream `getKey`: a stable key per entry, so a screen keeps its state while others
    /// are pushed on it. Defaults to the entry's position.
    pub fn get_key(mut self, f: impl Fn(&T, usize) -> SharedString + 'static) -> Self {
        self.get_key = Some(Rc::new(f));
        self
    }

    /// Upstream `isInteractivePopEnabled` (on by default): a sideways drag in the content,
    /// under any pointer, pops.
    pub fn interactive_pop_enabled(mut self, enabled: bool) -> Self {
        self.interactive_pop = enabled;
        self
    }

    /// Upstream `label`: the navigator region's accessible name.
    pub fn label(mut self, label: impl Into<SharedString>) -> Self {
        self.label = Some(label.into());
        self
    }
}

/// One screen's moving layers (upstream `ScreenLayers`).
struct Layers {
    card: FrameEngine,
    dim: FrameEngine,
    bar: FrameEngine,
    title: FrameEngine,
    veil: FrameEngine,
}

/// What a screen keeps while it is in the stack.
struct ScreenState {
    scroll: ScrollHandle,
    title_focus: FocusHandle,
    card_focus: FocusHandle,
    large_title_height: f64,
    collapsed: bool,
    veil_on: bool,
    title_shown: Option<bool>,
}

/// A transition (upstream `Transition`).
#[derive(Clone, Debug)]
struct Running {
    kind: Option<TransitionKind>,
    top: SharedString,
    under: Option<SharedString>,
    done: bool,
}

/// A back swipe in progress.
#[derive(Clone, Debug)]
struct Swipe {
    start: f64,
    top: SharedString,
    under: SharedString,
    past_dead_zone: bool,
}

/// What a stack keeps between frames.
struct StackState<T> {
    shown: Vec<SharedString>,
    path: Vec<T>,
    /// The popped top, rendered until its slide-out ends: its key, entry and index.
    leaving: Option<(SharedString, T, usize)>,
    running: Option<Running>,
    layers: HashMap<SharedString, Layers>,
    screens: HashMap<SharedString, ScreenState>,
    /// The control each pushed screen returns focus to.
    sources: HashMap<SharedString, Option<FocusHandle>>,
    pending_source: Option<Option<FocusHandle>>,
    /// The released back swipe's velocity, for the pop it asks for.
    release: Option<f64>,
    /// A pop whose focus waits for its slide-out to end: the popped key and the revealed.
    pending_focus: Option<PendingFocus>,
    progress: f64,
    swipe: Option<Swipe>,
    width: f64,
    direction: Direction,
    back: FrameEngine,
    can_pop: bool,
    on_path_change: Option<PathFn<T>>,
    top_under: Option<(SharedString, SharedString)>,
}

fn rd(spring: (f64, f64)) -> SpringDescription {
    SpringDescription::ResponseDamping {
        response: spring.0,
        damping_ratio: spring.1,
    }
}

impl<T: Clone + 'static> StackState<T> {
    fn layers(&mut self, key: &SharedString, cx: &App) -> &Layers {
        let executor = cx.background_executor().clone();
        self.layers.entry(key.clone()).or_insert_with(|| {
            let engine = |values| FrameEngine::new(executor.clone(), values);
            Layers {
                card: engine(MotionValues::default()),
                dim: engine(MotionValues::new().opacity(0.)),
                bar: engine(MotionValues::default()),
                title: engine(MotionValues::default()),
                veil: engine(MotionValues::new().opacity(0.)),
            }
        })
    }

    /// The layers at progress `p` at once (upstream `setProgress`).
    fn set_progress(
        &mut self,
        p: f64,
        top: &SharedString,
        under: Option<&SharedString>,
        ride: bool,
        cx: &App,
    ) {
        let values = layers_at(p, self.width, self.direction);
        let top_layers = self.layers(top, cx);
        top_layers.card.set(MotionValues::new().x(values.top_x));
        top_layers.bar.set(MotionValues::new().x(values.top_x));
        if let Some(under) = under {
            let l = self.layers(under, cx);
            l.card.set(MotionValues::new().x(values.under_x));
            l.bar
                .set(MotionValues::new().x(if ride { values.under_x } else { 0. }));
            l.dim.set(MotionValues::new().opacity(values.dim));
        }
        self.progress = p;
    }

    /// Springs the layers to `to` (upstream `animateProgress`).
    #[allow(clippy::too_many_arguments)]
    fn animate_progress(
        &mut self,
        to: f64,
        top: &SharedString,
        under: Option<&SharedString>,
        spring: SpringDescription,
        dp: Option<f64>,
        release: bool,
        ride: bool,
        cx: &App,
    ) {
        let (width, direction) = (self.width, self.direction);
        let values = layers_at(to, width, direction);
        let velocity = dp.map(|dp| layer_velocities(dp, width, direction));
        let transition = |v: Option<f64>, key: fn(MotionValues, f64) -> MotionValues| {
            let t = SpringTransition::new(spring);
            match v {
                Some(v) => t.velocity(key(MotionValues::new(), v)),
                None => t,
            }
        };
        let options = |fade: Option<CrossfadeAs>| AnimateOptions {
            touch_release: release,
            crossfade_as: fade,
            ..Default::default()
        };
        let x = |m: MotionValues, v: f64| m.x(v);
        let op = |m: MotionValues, v: f64| m.opacity(v);
        let fade_top = if to == 0. {
            CrossfadeAs::Entrance
        } else {
            CrossfadeAs::Exit
        };
        let fade_under = if to == 1. {
            CrossfadeAs::Entrance
        } else {
            CrossfadeAs::Exit
        };
        {
            let l = self.layers(top, cx);
            l.card.animate(
                MotionValues::new().x(values.top_x),
                transition(velocity.map(|v| v.top_x), x),
                options(Some(fade_top)),
            );
            l.bar.animate(
                MotionValues::new().x(values.top_x),
                transition(velocity.map(|v| v.top_x), x),
                options(None),
            );
        }
        if let Some(under) = under {
            let l = self.layers(under, cx);
            if ride {
                l.bar.animate(
                    MotionValues::new().x(values.under_x),
                    transition(velocity.map(|v| v.under_x), x),
                    options(None),
                );
            }
            l.card.animate(
                MotionValues::new().x(values.under_x),
                transition(velocity.map(|v| v.under_x), x),
                options(Some(fade_under)),
            );
            if l.dim.reduced(release) {
                l.dim.set(MotionValues::new().opacity(0.));
            } else {
                l.dim.animate(
                    MotionValues::new().opacity(values.dim),
                    transition(velocity.map(|v| v.dim), op),
                    options(None),
                );
            }
        }
        self.progress = to;
    }

    /// Puts a screen at rest on top or covered (upstream `settle`).
    fn settle(&mut self, key: &SharedString, top: bool, cx: &App) {
        let values = layers_at(0., self.width, self.direction);
        let l = self.layers(key, cx);
        if top {
            l.card.set(MotionValues::new().x(0.).opacity(1.));
            l.dim.set(MotionValues::new().opacity(0.));
            l.bar.set(MotionValues::new().x(0.));
        } else {
            l.card
                .set(MotionValues::new().x(values.under_x).opacity(1.));
            l.dim.set(MotionValues::new().opacity(1.));
            l.bar.set(MotionValues::new().x(values.under_x));
        }
    }

    fn involved_at_rest(&self, running: &Running) -> bool {
        [Some(&running.top), running.under.as_ref()]
            .into_iter()
            .flatten()
            .filter_map(|k| self.layers.get(k))
            .all(|l| l.card.at_rest() && l.dim.at_rest() && l.bar.at_rest())
    }
}

/// Moves focus after a transition (upstream `focusAfter`): only when focus was lost
/// (nothing focused, or focus inside a covered screen), and not when it is already in the
/// screen; a pop returns it to the control that pushed, else to the title.
fn focus_after<T: Clone + 'static>(
    state: &Entity<StackState<T>>,
    pop: Option<Option<FocusHandle>>,
    key: &SharedString,
    revealed: Option<&SharedString>,
    lost_already: bool,
    window: &mut Window,
    cx: &mut App,
) {
    let target = revealed.unwrap_or(key).clone();
    let (target_screen, others) = {
        let s = state.read(cx);
        let target_screen = s
            .screens
            .get(&target)
            .map(|sc| (sc.card_focus.clone(), sc.title_focus.clone()));
        let others: Vec<FocusHandle> = s
            .screens
            .iter()
            .filter(|(k, _)| **k != target)
            .map(|(_, sc)| sc.card_focus.clone())
            .collect();
        (target_screen, others)
    };
    let Some((card, title)) = target_screen else {
        return;
    };
    let focused = window.focused(cx);
    let lost =
        lost_already || focused.is_none() || others.iter().any(|f| f.contains_focused(window, cx));
    if !lost || card.contains_focused(window, cx) {
        return;
    }
    // A pop returns focus to the control that pushed, while it is still there.
    if let Some(Some(source)) = pop {
        window.focus(&source, cx);
        if source.is_focused(window) {
            return;
        }
    }
    window.focus(&title, cx);
}

fn back_swipe<T: Clone + 'static>(
    state: &Entity<StackState<T>>,
    window: &mut Window,
    cx: &mut App,
) -> (GestureAttachment, PanGestureRecognizer) {
    let rtl = state.read(cx).direction == Direction::Rtl;
    let pan = PanGestureRecognizer::new(PanOptions {
        direction: if rtl {
            PanDirection::Left
        } else {
            PanDirection::Right
        },
        maximum_number_of_touches: 1,
        scroll_edge: true,
        ..PanOptions::named("NavigationStack.contentBackSwipe")
    });
    // The pan claims the touch after its own hysteresis; the content swipe begins once the
    // finger has travelled UIKit's 27 px.
    let dead_zone = (CONTENT_POP_HYSTERESIS - pan.hysteresis()).max(0.);
    let weak: WeakEntity<StackState<T>> = state.downgrade();
    pan.add_listener(move |r| {
        let (phase, translation, velocity) = (r.state(), r.translation().x, r.velocity().x);
        let weak = weak.clone();
        after_gesture(move |window, cx| {
            let Some(state) = weak.upgrade() else { return };
            let finish = state.update(cx, |s, cx| {
                if !s.can_pop {
                    return None;
                }
                let (top, under) = s.top_under.clone()?;
                let sign = if s.direction == Direction::Rtl {
                    -1.
                } else {
                    1.
                };
                match phase {
                    GestureState::Began => {
                        // Grab the cards where they show.
                        let x = s.layers(&top, cx).card.get(MotionProperty::X, cx);
                        for key in [&top, &under] {
                            let l = s.layers(key, cx);
                            l.card.stop();
                            l.dim.stop();
                            l.bar.stop();
                        }
                        s.running = None;
                        let start = progress_from_screen(x, s.width, s.direction);
                        s.swipe = Some(Swipe {
                            start,
                            top: top.clone(),
                            under: under.clone(),
                            past_dead_zone: dead_zone <= 0.,
                        });
                        s.set_progress(start, &top, Some(&under), false, cx);
                        cx.notify();
                        None
                    }
                    GestureState::Changed => {
                        let swipe = s.swipe.as_mut()?;
                        let along = sign * translation;
                        if along >= dead_zone {
                            swipe.past_dead_zone = true;
                        }
                        let travel = if swipe.past_dead_zone {
                            along - dead_zone
                        } else {
                            0.
                        };
                        let (start, t, u) = (swipe.start, swipe.top.clone(), swipe.under.clone());
                        let p = progress_from_travel(start, sign * travel, s.width, s.direction);
                        s.set_progress(p, &t, Some(&u), false, cx);
                        cx.notify();
                        None
                    }
                    GestureState::Ended | GestureState::Cancelled => {
                        let swipe = s.swipe.take()?;
                        let p = s.progress;
                        let v = if phase == GestureState::Ended {
                            pop_velocity(velocity, s.width, s.direction)
                        } else {
                            0.
                        };
                        let finish = phase == GestureState::Ended && should_finish_pop(p, v);
                        if !finish {
                            let spring = if p < 0. {
                                rd(INTERACTIVE_POP_RETURN)
                            } else {
                                NAVIGATION_INTERACTIVE_POP_FINISH
                            };
                            s.animate_progress(
                                0.,
                                &swipe.top,
                                Some(&swipe.under),
                                spring,
                                Some(v),
                                true,
                                false,
                                cx,
                            );
                            s.running = Some(Running {
                                kind: None,
                                top: swipe.top.clone(),
                                under: Some(swipe.under.clone()),
                                done: false,
                            });
                            cx.notify();
                            return None;
                        }
                        s.release = Some(v);
                        Some((s.path.clone(), s.on_path_change.clone()))
                    }
                    _ => None,
                }
            });
            if let Some((path, Some(on_change))) = finish {
                let mut next = path;
                next.pop();
                on_change(next, window, cx);
            }
        });
    });
    let attachment = gesture_host(window, cx).attach(&[pan.any()]);
    (attachment, pan)
}

/// The back button's label (upstream `backLabelFor`): the screen underneath's back
/// title, else its title, else the catalog's "Back".
fn back_label(previous: Option<&NavigationScreen>, cx: &App) -> SharedString {
    previous
        .and_then(|p| {
            p.back_title
                .clone()
                .filter(|t| !t.is_empty())
                .or_else(|| Some(p.title.clone()).filter(|t| !t.is_empty()))
        })
        .unwrap_or_else(|| i18n(cx).t("@astryx.navigationStack.back"))
}

impl<T: Clone + 'static> RenderOnce for NavigationStack<T> {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = current(window, cx);
        let t = theme.tokens().clone();
        let rem = window.rem_size();
        let rtl = use_direction(cx).is_rtl();
        let touch = coarse_primary_pointer(window, cx);
        let direction = if rtl { Direction::Rtl } else { Direction::Ltr };
        let render_screen = self.render_screen.clone();
        let key_of = |entry: &T, index: usize| -> SharedString {
            match &self.get_key {
                Some(f) => format!("screen-{}", f(entry, index)).into(),
                None => format!("screen-{index}").into(),
            }
        };
        let mut keys: Vec<SharedString> = vec!["root".into()];
        keys.extend(self.path.iter().enumerate().map(|(i, e)| key_of(e, i)));

        let state = {
            let keys = keys.clone();
            let path = self.path.clone();
            let executor = cx.background_executor().clone();
            window.use_keyed_state(
                ElementId::Name(format!("{:?}-navigation-stack", self.id).into()),
                cx,
                move |_, _| StackState {
                    shown: keys,
                    path,
                    leaving: None,
                    running: None,
                    layers: HashMap::new(),
                    screens: HashMap::new(),
                    sources: HashMap::new(),
                    pending_source: None,
                    release: None,
                    pending_focus: None,
                    progress: 0.,
                    swipe: None,
                    width: 0.,
                    direction: Direction::Ltr,
                    back: FrameEngine::new(executor, MotionValues::default()),
                    can_pop: false,
                    on_path_change: None,
                    top_under: None,
                },
            )
        };

        // A change of path: a push adds one key at the end, a pop removes from the end.
        let mut focus_push: Option<SharedString> = None;
        state.update(cx, |s, cx| {
            s.direction = direction;
            s.on_path_change = self.on_path_change.clone();
            if s.shown != keys {
                let before = std::mem::replace(&mut s.shown, keys.clone());
                let old_path = std::mem::replace(&mut s.path, self.path.clone());
                let is_prefix = |shorter: &[SharedString], longer: &[SharedString]| {
                    shorter.iter().zip(longer).all(|(a, b)| a == b) && shorter.len() <= longer.len()
                };
                let was_running = s.running.as_ref().is_some_and(|r| !r.done);
                let release = s.release.take();
                s.pending_focus = None;
                if keys.len() == before.len() + 1 && is_prefix(&before, &keys) {
                    let top = keys.last().cloned().unwrap_or_default();
                    let under = before.last().cloned();
                    if let Some(source) = s.pending_source.take() {
                        s.sources.insert(top.clone(), source);
                    }
                    s.leaving = None;
                    if !was_running {
                        s.set_progress(1., &top, under.as_ref(), false, cx);
                    } else {
                        let values = layers_at(1., s.width, s.direction);
                        let l = s.layers(&top, cx);
                        l.card.set(MotionValues::new().x(values.top_x));
                        l.bar.set(MotionValues::new().x(values.top_x));
                    }
                    let dp = (!was_running).then(|| programmatic_velocity(TransitionKind::Push));
                    s.animate_progress(
                        0.,
                        &top,
                        under.as_ref(),
                        NAVIGATION_PUSH,
                        dp,
                        false,
                        true,
                        cx,
                    );
                    // On a push from the root the chevron fades in after the screens start.
                    if under.as_deref() == Some("root") {
                        s.back.set(MotionValues::new().opacity(0.));
                        s.back.animate(
                            MotionValues::new().opacity(1.),
                            SpringTransition::new(rd(BACK_BUTTON)),
                            AnimateOptions {
                                at: Some(s.back.now(cx) + BACK_CHEVRON_PUSH_DELAY_MS),
                                ..Default::default()
                            },
                        );
                    }
                    s.running = Some(Running {
                        kind: Some(TransitionKind::Push),
                        top,
                        under,
                        done: false,
                    });
                } else if keys.len() < before.len() && is_prefix(&keys, &before) {
                    let top = before.last().cloned().unwrap_or_default();
                    let under = keys.last().cloned();
                    let index = before.len() - 2;
                    if let Some(entry) = old_path.get(index).cloned() {
                        s.leaving = Some((top.clone(), entry, index));
                    }
                    match release {
                        Some(v) => s.animate_progress(
                            1.,
                            &top,
                            under.as_ref(),
                            NAVIGATION_INTERACTIVE_POP_FINISH,
                            Some(v),
                            true,
                            false,
                            cx,
                        ),
                        None => {
                            if !was_running {
                                s.set_progress(0., &top, under.as_ref(), true, cx);
                            }
                            let dp =
                                (!was_running).then(|| programmatic_velocity(TransitionKind::Pop));
                            s.animate_progress(
                                1.,
                                &top,
                                under.as_ref(),
                                NAVIGATION_POP,
                                dp,
                                false,
                                true,
                                cx,
                            );
                        }
                    }
                    if under.as_deref() == Some("root") {
                        s.back.animate(
                            MotionValues::new().opacity(0.),
                            SpringTransition::new(rd(BACK_BUTTON)),
                            AnimateOptions::default(),
                        );
                    }
                    s.running = Some(Running {
                        kind: Some(TransitionKind::Pop),
                        top,
                        under,
                        done: false,
                    });
                } else {
                    s.leaving = None;
                    s.running = None;
                    let n = keys.len();
                    for (i, key) in keys.iter().enumerate() {
                        s.settle(key, i + 1 == n, cx);
                    }
                    s.back.set(MotionValues::new().opacity(1.));
                }
            }
            s.path = self.path.clone();
            // Drive the motion; a transition whose layers rest is done.
            for l in s.layers.values() {
                l.card.sync(window, cx);
                l.dim.sync(window, cx);
                l.bar.sync(window, cx);
            }
            if let Some(running) = s.running.clone()
                && !running.done
                && s.involved_at_rest(&running)
            {
                if let Some(r) = s.running.as_mut() {
                    r.done = true;
                }
                match running.kind {
                    Some(TransitionKind::Pop) => {
                        s.leaving = None;
                        if let Some(under) = &running.under {
                            s.settle(under, true, cx);
                            let source = s.sources.remove(&running.top).flatten();
                            let popped = s
                                .screens
                                .get(&running.top)
                                .map(|sc| (sc.card_focus.clone(), sc.title_focus.clone()));
                            s.pending_focus =
                                Some((running.top.clone(), under.clone(), source, popped));
                        }
                    }
                    Some(TransitionKind::Push) => focus_push = Some(running.top.clone()),
                    None => {
                        // A swipe that sprang back.
                        s.settle(&running.top, true, cx);
                        if let Some(under) = &running.under {
                            s.settle(under, false, cx);
                        }
                    }
                }
            }
            // Screens no longer in the stack drop their state.
            let leaving = s.leaving.as_ref().map(|(k, _, _)| k.clone());
            s.screens
                .retain(|k, _| keys.contains(k) || leaving.as_ref() == Some(k));
            s.layers
                .retain(|k, _| keys.contains(k) || leaving.as_ref() == Some(k));
            s.sources
                .retain(|k, _| keys.contains(k) || leaving.as_ref() == Some(k));
        });
        if let Some(top) = focus_push {
            focus_after(&state, None, &top, None, false, window, cx);
        }
        let pending = state.update(cx, |s, _| {
            if s.leaving.is_none() {
                s.pending_focus.take()
            } else {
                None
            }
        });
        if let Some((popped, revealed, source, popped_card)) = pending {
            // Focus inside the popped screen, which is going, is lost.
            let in_popped = popped_card.is_some_and(|(card, title)| {
                card.contains_focused(window, cx) || title.is_focused(window)
            });
            focus_after(
                &state,
                Some(source),
                &popped,
                Some(&revealed),
                in_popped,
                window,
                cx,
            );
        }

        // The screens: the models, then the leaving screen.
        let leaving = state.read(cx).leaving.clone();
        let mut screens: Vec<(SharedString, NavigationScreen)> = vec![("root".into(), self.root)];
        if let Some(f) = &render_screen {
            for (i, entry) in self.path.iter().enumerate() {
                screens.push((keys[i + 1].clone(), f(entry, i, window, cx)));
            }
            if let Some((key, entry, index)) = &leaving {
                screens.push((key.clone(), f(entry, *index, window, cx)));
            }
        }
        let top_key = screens.last().map(|(k, _)| k.clone()).unwrap_or_default();
        let under_key: Option<SharedString> = if leaving.is_some() {
            state
                .read(cx)
                .running
                .as_ref()
                .and_then(|r| r.under.clone())
        } else {
            screens.len().checked_sub(2).map(|i| screens[i].0.clone())
        };
        let depth = self.path.len();
        let can_pop = depth > 0 && leaving.is_none();
        state.update(cx, |s, _| {
            s.can_pop = can_pop && self.interactive_pop;
            s.top_under = (depth > 0).then(|| (keys[depth].clone(), keys[depth - 1].clone()));
        });

        // The API screens push and pop through.
        let api: NavigationStackApi<T> = {
            let (push_state, path, on_change) = (
                state.clone(),
                self.path.clone(),
                self.on_path_change.clone(),
            );
            let (pop_path, pop_change) = (self.path.clone(), self.on_path_change.clone());
            let (root_path, root_change) = (self.path.clone(), self.on_path_change.clone());
            NavigationStackApi {
                push: Rc::new(move |entry, source, window, cx| {
                    push_state.update(cx, |s, _| s.pending_source = Some(source));
                    if let Some(f) = &on_change {
                        let mut next = path.clone();
                        next.push(entry);
                        f(next, window, cx);
                    }
                }),
                pop: Rc::new(move |window, cx| {
                    if !pop_path.is_empty()
                        && let Some(f) = &pop_change
                    {
                        f(pop_path[..pop_path.len() - 1].to_vec(), window, cx);
                    }
                }),
                pop_to_root: Rc::new(move |window, cx| {
                    if !root_path.is_empty()
                        && let Some(f) = &root_change
                    {
                        f(Vec::new(), window, cx);
                    }
                }),
                depth,
            }
        };

        let (swipe_attachment, _) = {
            let state = state.clone();
            let gestures = window.use_keyed_state(
                ElementId::Name(format!("{:?}-navigation-swipe", self.id).into()),
                cx,
                move |window, cx| back_swipe(&state, window, cx),
            );
            let read = gestures.read(cx);
            (read.0.clone(), ())
        };

        // Draw.
        let mut moving = false;
        let back_index = screens.iter().position(|(k, _)| *k == top_key);
        let top_back_label = back_index
            .filter(|i| *i > 0)
            .map(|i| back_label(Some(&screens[i - 1].1), cx));
        let body = t.color.background_body;
        #[cfg(test)]
        let mut drawn_screens: Vec<DrawnScreen> = Vec::new();
        let mut cards = Vec::new();
        let mut bars = Vec::new();
        let width = state.read(cx).width;
        let clip_x = state.update(cx, |s, cx| {
            s.layers(&top_key, cx).card.get(MotionProperty::X, cx)
        });
        for (key, screen) in screens {
            let is_top = key == top_key && leaving.is_none();
            let is_covered = key != top_key || leaving.is_some();
            let is_hidden = key != top_key && Some(&key) != under_key.as_ref();
            let large = screen.title_display_mode == TitleDisplayMode::Large;
            let (scroll, title_focus, card_focus, values) = state.update(cx, |s, cx| {
                let screen_state = s.screens.entry(key.clone()).or_insert_with(|| ScreenState {
                    scroll: ScrollHandle::new(),
                    title_focus: cx.focus_handle(),
                    card_focus: cx.focus_handle(),
                    large_title_height: 0.,
                    collapsed: false,
                    veil_on: false,
                    title_shown: None,
                });
                // The large title has collapsed once it has scrolled entirely under the bar.
                let top_offset = rustryx_primitives::keyboard::scroll_top_of(&screen_state.scroll);
                screen_state.collapsed = large
                    && screen_state.large_title_height > 0.
                    && top_offset >= screen_state.large_title_height - BAR_HEIGHT;
                // The veil switches on once the content has moved 10 px under the bar (past
                // the large title, on a large screen).
                let switch = if large {
                    screen_state.large_title_height
                } else {
                    SCROLL_EDGE_SWITCH
                };
                let veil_on = !is_hidden && top_offset > switch;
                let veil_changed = screen_state.veil_on != veil_on;
                screen_state.veil_on = veil_on;
                let title_shown = !(large && !screen_state.collapsed);
                let title_changed = screen_state.title_shown.is_some_and(|w| w != title_shown);
                let first_title = screen_state.title_shown.is_none();
                screen_state.title_shown = Some(title_shown);
                let (scroll, title_focus, card_focus) = (
                    screen_state.scroll.clone(),
                    screen_state.title_focus.clone(),
                    screen_state.card_focus.clone(),
                );
                let l = s.layers(&key, cx);
                if veil_changed {
                    let (m, k, d) = if veil_on {
                        SCROLL_EDGE_APPEAR
                    } else {
                        SCROLL_EDGE_DISAPPEAR
                    };
                    l.veil.animate(
                        MotionValues::new().opacity(if veil_on { 1. } else { 0. }),
                        SpringTransition::new(SpringDescription::Physics {
                            mass: m,
                            stiffness: k,
                            damping: d,
                        }),
                        AnimateOptions::default(),
                    );
                }
                let title_target = MotionValues::new()
                    .opacity(if title_shown { 1. } else { 0. })
                    .y(if title_shown { 0. } else { INLINE_TITLE_RISE });
                if first_title {
                    l.title.set(title_target);
                } else if title_changed {
                    l.title.animate(
                        title_target,
                        SpringTransition::new(NAVIGATION_BAR_ELEMENTS),
                        AnimateOptions::default(),
                    );
                }
                let mut m = false;
                for e in [&l.card, &l.dim, &l.bar, &l.title, &l.veil] {
                    m |= e.drive(cx);
                }
                let values = (
                    l.card.get(MotionProperty::X, cx),
                    l.card.shown_opacity(cx),
                    l.dim.get(MotionProperty::Opacity, cx),
                    l.bar.get(MotionProperty::X, cx),
                    l.title.get(MotionProperty::Opacity, cx),
                    l.title.get(MotionProperty::Y, cx),
                    l.veil.get(MotionProperty::Opacity, cx),
                    m,
                );
                (scroll, title_focus, card_focus, values)
            });
            let (card_x, card_opacity, dim, bar_x, title_opacity, title_y, veil, m) = values;
            #[cfg(test)]
            drawn_screens.push(DrawnScreen {
                key: key.clone(),
                title: screen.title.clone(),
                hidden: is_hidden,
                covered: is_covered,
                card_x,
                title_opacity,
                title_focus: title_focus.clone(),
                large,
            });
            moving |= m;

            // The card.
            let mut content = div().pt(px(BAR_HEIGHT as f32)).flex().flex_col();
            if large {
                let probe = state.downgrade();
                let probe_key = key.clone();
                content = content.child(
                    div()
                        .id(ElementId::Name(format!("{key}-large-title").into()))
                        .relative()
                        // The large title takes the title's focus while the bar's title hides.
                        .when(!title_shown_now(title_opacity), |d| {
                            d.track_focus(&title_focus)
                        })
                        .role(Role::Heading)
                        .px(t.spacing.s4)
                        .pt(t.spacing.s1)
                        .pb(t.spacing.s2)
                        .text_size(t.font.size.s4xl)
                        .font_weight(t.font.weight.bold)
                        .child(screen.title.clone())
                        .child(
                            canvas(
                                move |bounds: Bounds<Pixels>, _, cx| {
                                    let _ = probe.update(cx, |s, _| {
                                        if let Some(sc) = s.screens.get_mut(&probe_key) {
                                            sc.large_title_height = f64::from(bounds.size.height);
                                        }
                                    });
                                },
                                |_, _, _, _| {},
                            )
                            .absolute()
                            .inset_0(),
                        ),
                );
            }
            let content = content.children(screen.children);
            let selector = format!("{}-{key}-card", self.id);
            let scroller = div()
                .id(ElementId::Name(
                    format!("{:?}-{key}-scroller", self.id).into(),
                ))
                .absolute()
                .inset_0()
                .overflow_y_scroll()
                .track_scroll(&scroll)
                .child(content);
            let veil_el = div()
                .absolute()
                .top_0()
                .left_0()
                .right_0()
                .opacity(veil as f32)
                .child(
                    div()
                        .h(px(
                            (BAR_HEIGHT - VEIL_EDGE_ABOVE_BOTTOM - VEIL_SOFTNESS).max(0.) as f32,
                        ))
                        .bg(body.opacity(VEIL_ALPHA as f32)),
                )
                .child(
                    div()
                        .h(px(
                            (2. * VEIL_SOFTNESS + SCROLL_EDGE_SOFT_EXTENT - BAR_HEIGHT)
                                .max(VEIL_SOFTNESS * 2.) as f32,
                        ))
                        .bg(linear_gradient(
                            180.,
                            linear_color_stop(body.opacity((VEIL_ALPHA * 0.84) as f32), 0.),
                            linear_color_stop(body.opacity(0.), 1.),
                        )),
                );
            let dim_el = div()
                .absolute()
                .inset_0()
                .bg(Hsla::black().opacity((DIMMING_ALPHA * dim) as f32));
            let mut card = div()
                .id(ElementId::Name(format!("{:?}-{key}-card", self.id).into()))
                .debug_selector(move || selector)
                .track_focus(&card_focus)
                .absolute()
                .inset_0()
                .overflow_hidden()
                .bg(body)
                .translate_x(px(card_x as f32))
                .opacity(card_opacity as f32)
                .child(scroller)
                .child(veil_el)
                .child(dim_el);
            if is_hidden {
                // Covered screens beyond the one underneath: kept, but not drawn.
                card = card.hidden();
            }
            // A covered screen is inert: out of the tab order, the pointer and the
            // accessibility tree.
            card = card.inert(is_covered);
            let _ = is_top;
            cards.push(card.into_any_element());

            // The screen's bar layer, which moves with its card.
            if !is_hidden {
                let title_selector = format!("{}-{key}-title", self.id);
                let mut row = div()
                    .absolute()
                    .inset_0()
                    .flex()
                    .items_center()
                    .px(t.spacing.s4)
                    .child(div().flex_1().min_w_0())
                    .child(
                        div()
                            .id(ElementId::Name(
                                format!("{:?}-{key}-bar-title", self.id).into(),
                            ))
                            .debug_selector(move || title_selector)
                            .when(!large || title_shown_now(title_opacity), |d| {
                                d.track_focus(&title_focus)
                            })
                            // Hidden from assistive technology while the large title shows.
                            .inert(large && !title_shown_now(title_opacity))
                            .role(Role::Heading)
                            .min_w_0()
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .text_ellipsis()
                            .text_size(t.font.size.lg)
                            .font_weight(t.font.weight.semibold)
                            .opacity(title_opacity as f32)
                            .translate_y(px(title_y as f32))
                            .child(screen.title.clone()),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .justify_end()
                            .children(screen.end_content),
                    );
                if !touch {
                    row = row.child(
                        div()
                            .absolute()
                            .left_0()
                            .right_0()
                            .top_0()
                            .bottom(px(1.))
                            .bg(t.color.background_surface)
                            .opacity(0.),
                    );
                }
                let mut layer = div().absolute().inset_0().translate_x(px(bar_x as f32));
                if !touch {
                    // Each bar has an opaque background of its own, so the incoming one covers
                    // the outgoing title.
                    layer = layer.child(
                        div()
                            .absolute()
                            .left_0()
                            .right_0()
                            .top_0()
                            .bottom(px(1.))
                            .bg(t.color.background_surface),
                    );
                }
                layer = layer.child(row).child(
                    div()
                        .absolute()
                        .left_0()
                        .right_0()
                        .top_0()
                        .bottom(px(1.))
                        .bg(Hsla::black().opacity((DIMMING_ALPHA * dim) as f32)),
                );
                let layer = if touch && is_covered && Some(&key) == under_key.as_ref() {
                    // The bars are clear: the covered bar is cut at the top card's leading edge.
                    let visible = if rtl { width - clip_x } else { clip_x };
                    div()
                        .absolute()
                        .top_0()
                        .bottom_0()
                        .w(px(visible.max(0.) as f32))
                        .when(rtl, |d| d.right_0())
                        .overflow_hidden()
                        .child(
                            div()
                                .absolute()
                                .top_0()
                                .bottom_0()
                                .w(px(width as f32))
                                .when(rtl, |d| d.right_0())
                                .child(layer),
                        )
                } else {
                    layer
                };
                let layer = layer.inert(is_covered);
                bars.push(layer.into_any_element());
            }
        }
        // The top bar first: the covered one draws under it.
        bars.reverse();
        #[cfg(test)]
        let back_label_for_test = top_back_label.clone();

        // The back button: one for the stack, over the moving bars.
        let back = top_back_label.map(|label| {
            let back_id = ElementId::Name(format!("{:?}-back", self.id).into());
            let focus = control_focus(&back_id, window, cx);
            let chevron = state.update(cx, |s, cx| {
                moving |= s.back.drive(cx);
                s.back.get(MotionProperty::Opacity, cx)
            });
            let pop = api.pop.clone();
            let inert = leaving.is_some();
            let mut button = div()
                .id(back_id)
                .debug_selector(|| "navigation-back".into())
                .track_focus(&focus)
                .size(px(44.))
                .rounded_full()
                .border_1()
                .border_color(t.color.border)
                .bg(t.color.background_surface)
                .text_color(t.color.text_accent)
                .flex()
                .items_center()
                .justify_center()
                .cursor_pointer()
                .role(Role::Button)
                .aria_label(label)
                .child(
                    div().opacity(chevron as f32).child(
                        Icon::new(if rtl { "chevronRight" } else { "chevronLeft" })
                            .color(IconColor::Inherit),
                    ),
                );
            button = button.inert(inert);
            if !inert {
                button = button.on_click(move |_, window, cx| {
                    cx.stop_propagation();
                    pop(window, cx);
                });
            }
            if is_focus_visible(&focus, window) {
                button = button.child(rustryx_components::focus::focus_ring(&t, px(9999.), rem));
            }
            div()
                .absolute()
                .top_0()
                .bottom_0()
                .when(!rtl, |d| d.left(t.spacing.s4))
                .when(rtl, |d| d.right(t.spacing.s4))
                .flex()
                .items_center()
                .child(button)
                .into_any_element()
        });
        #[cfg(test)]
        DRAWN.with(|d| {
            d.borrow_mut().insert(
                self.id.to_string(),
                Drawn {
                    screens: drawn_screens,
                    back: back_label_for_test.clone(),
                    leaving: leaving.as_ref().map(|(k, _, _)| k.clone()),
                },
            );
        });
        if moving {
            window.request_animation_frame();
        }

        let probe = state.downgrade();
        let measure = canvas(
            move |bounds: Bounds<Pixels>, window, cx| {
                let _ = probe.update(cx, |s, _| {
                    let w = f64::from(bounds.size.width);
                    if s.width != w {
                        s.width = w;
                        window.refresh();
                    }
                });
            },
            |_, _, _, _| {},
        )
        .absolute()
        .inset_0();
        let mut bar_background = div().absolute().inset_0();
        if !touch {
            bar_background = bar_background
                .bg(t.color.background_surface)
                .border_b_1()
                .border_color(t.color.border);
        }
        let bar = div()
            .absolute()
            .top_0()
            .left_0()
            .right_0()
            .h(px(BAR_HEIGHT as f32))
            .child(bar_background)
            .children(bars.into_iter().rev())
            .children(back);
        // The swipe's area lies over the cards: a covered card's inert layer would hide an
        // area under it. Its hitbox takes no pointer from the content below.
        let cards = div().absolute().inset_0().children(cards);
        let swipe_area = gesture_area(&swipe_attachment, div().absolute().inset_0());
        let selector = format!("{}", self.id);
        let mut stack = div()
            .id(self.id.clone())
            .debug_selector(move || selector)
            .relative()
            .size_full()
            .overflow_hidden()
            .bg(body)
            .role(Role::Region)
            .child(measure)
            .child(cards)
            .child(swipe_area)
            .child(bar);
        if let Some(label) = self.label {
            stack = stack.aria_label(label);
        }
        theme.refine(stack.style(), "navigation-stack", &[], None);
        provide(api, stack)
    }
}

#[cfg(test)]
thread_local! {
    /// What each stack drew last, by its id, for tests.
    pub(crate) static DRAWN: std::cell::RefCell<HashMap<String, Drawn>> = std::cell::RefCell::default();
}

/// What a stack drew, for tests.
#[cfg(test)]
#[derive(Clone)]
pub(crate) struct Drawn {
    pub screens: Vec<DrawnScreen>,
    pub back: Option<SharedString>,
    pub leaving: Option<SharedString>,
}

/// One screen as drawn, for tests.
#[cfg(test)]
#[derive(Clone)]
pub(crate) struct DrawnScreen {
    pub key: SharedString,
    pub title: SharedString,
    pub hidden: bool,
    pub covered: bool,
    pub card_x: f64,
    pub title_opacity: f64,
    pub title_focus: FocusHandle,
    pub large: bool,
}

/// The bar's title takes focus while it shows.
fn title_shown_now(opacity: f64) -> bool {
    opacity > 0.5
}

const fn p(prop: &'static str, rust: RustApi) -> PropMapping {
    PropMapping { prop, rust }
}

/// Upstream lab `NavigationStack`.
pub const NAVIGATION_STACK_DOC: ComponentDoc = ComponentDoc {
    name: "NavigationStack",
    rust_type: "rustryx::lab::navigation_stack::NavigationStack",
    props: &[
        p("root", RustApi::Argument("root")),
        p("path", RustApi::Argument("path")),
        p("onPathChange", RustApi::Method("on_path_change")),
        p("renderScreen", RustApi::Method("render_screen")),
        p("getKey", RustApi::Method("get_key")),
        p(
            "isInteractivePopEnabled",
            RustApi::Method("interactive_pop_enabled"),
        ),
        p("label", RustApi::Method("label")),
    ],
    example: include_str!("examples.rs"),
};

/// Upstream lab `NavigationScreen`.
pub const NAVIGATION_SCREEN_DOC: ComponentDoc = ComponentDoc {
    name: "NavigationScreen",
    rust_type: "rustryx::lab::navigation_stack::NavigationScreen",
    props: &[
        p("title", RustApi::Argument("title")),
        p("backTitle", RustApi::Method("back_title")),
        p("titleDisplayMode", RustApi::Method("title_display_mode")),
        p("endContent", RustApi::Method("end_content")),
        p("children", RustApi::Trait("ParentElement")),
    ],
    example: include_str!("examples.rs"),
};
