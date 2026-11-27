//! `Tour`: a guided product tour (upstream lab `Tour`, component:Tour). The controller
//! draws no chrome: it owns the active step (advance, retreat, complete, dismiss) and
//! shares it with the [`TourStep`]s nested in it, whose order is the order they first
//! render in (upstream: registration on mount).
//!
//! - Each step anchors a non-modal callout to a ring drawn over its [`TourTarget`];
//!   the target keeps its own styles and behavior.
//! - Without a backdrop a press on the page or the target leaves the tour running, and
//!   Tab can leave the callout for the page (FR1, AR1). With one the page dims around the
//!   target and a press anywhere outside the callout ends the tour with `Backdrop`.
//! - Escape ends the tour through the layer stack. Focus moves into each step's callout
//!   as it shows, and back to the target (when it takes focus) or to what had focus when
//!   the tour started, when the tour ends with focus in the callout.
//! - Deactivating the tour resets it to its first step in the same frame.

mod step;
mod target;

pub mod examples;

#[cfg(test)]
mod tests;

use std::cell::RefCell;
use std::rc::Rc;

use gpui::prelude::*;
use gpui::{AnyElement, AnyWindowHandle, App, ElementId, Entity, FocusHandle, Window, canvas, div};
use rustryx_components::doc::{ComponentDoc, PropMapping, RustApi};
use rustryx_primitives::{provide, use_context};

pub use step::{HIGHLIGHT_PADDING, TourStep, labels, step_count_text};
use target::ReturnTarget;
pub use target::{TourTarget, TourTargetElement};

/// Why a tour was dismissed (upstream `TourDismissSource`), so a consumer can tell a
/// finished tour from an early exit.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TourDismissSource {
    /// A press on the dimmed background.
    Backdrop,
    /// Escape, at any step.
    Escape,
    /// The step's close control.
    Close,
    /// "Skip" or "maybe later" on the first step (a custom step UI's, through
    /// [`use_tour`]).
    Skip,
    /// Advancing past the final step.
    Complete,
}

impl TourDismissSource {
    pub fn as_str(self) -> &'static str {
        match self {
            TourDismissSource::Backdrop => "backdrop",
            TourDismissSource::Escape => "escape",
            TourDismissSource::Close => "close",
            TourDismissSource::Skip => "skip",
            TourDismissSource::Complete => "complete",
        }
    }
}

type DismissFn = Rc<dyn Fn(TourDismissSource, &mut Window, &mut App)>;
type StepFn = Rc<dyn Fn(&mut Window, &mut App)>;

/// The controller's state.
#[derive(Default)]
pub(crate) struct TourState {
    /// The registered steps, in registration order (upstream `stepIds`).
    step_ids: Vec<ElementId>,
    /// Upstream `activeStepIndex`.
    active_index: usize,
    /// `active` last frame.
    was_active: bool,
    /// A step showed last frame.
    was_running: bool,
    /// What had focus when the tour started (upstream `openerRef`).
    pub(crate) opener: Option<FocusHandle>,
    /// The active step's callout region, last frame.
    pub(crate) region: Option<FocusHandle>,
    /// Where focus returns from the active step: its target when it takes focus, else
    /// the opener.
    pub(crate) return_target: Option<ReturnTarget>,
    /// At the last dismissal: whether focus was in the callout, and what had it.
    focus_in_callout: bool,
    focused: Option<FocusHandle>,
}

/// What a Tour shares with its steps (upstream `TourContextValue`).
#[derive(Clone)]
pub(crate) struct TourContext {
    pub(crate) state: Entity<TourState>,
    /// The steps rendered this frame, in order.
    pub(crate) frame: Rc<RefCell<Vec<ElementId>>>,
    pub(crate) active_step: Option<ElementId>,
    pub(crate) active_index: usize,
    pub(crate) step_count: usize,
    pub(crate) step_count_shown: bool,
    pub(crate) backdrop: bool,
    pub(crate) next: StepFn,
    pub(crate) previous: StepFn,
    pub(crate) dismiss: DismissFn,
}

/// Controller for a guided product tour (upstream lab `Tour`).
///
/// ```ignore
/// let save = TourTarget::new("save");
/// div()
///     .child(save.wrap(Button::new("save", "Save")).control("save"))
///     .child(
///         Tour::new("onboarding", active, |source, _, cx| ..)
///             .backdrop(true)
///             .step_count_shown(true)
///             .child(TourStep::new("save-step", &save, "Save your work").child("Changes save automatically.")),
///     )
/// ```
#[derive(IntoElement)]
pub struct Tour {
    id: ElementId,
    active: bool,
    on_dismiss: DismissFn,
    backdrop: bool,
    step_count_shown: bool,
    children: Vec<AnyElement>,
}

impl Tour {
    /// A tour, running while `active` (upstream `isActive`: when false nothing shows
    /// and the tour resets to its first step), that reports every exit with its reason
    /// to `on_dismiss` (upstream `onDismiss`, `Complete` included); the consumer turns
    /// `active` off in response.
    pub fn new(
        id: impl Into<ElementId>,
        active: bool,
        on_dismiss: impl Fn(TourDismissSource, &mut Window, &mut App) + 'static,
    ) -> Self {
        Tour {
            id: id.into(),
            active,
            on_dismiss: Rc::new(on_dismiss),
            backdrop: false,
            step_count_shown: false,
            children: Vec::new(),
        }
    }

    /// Upstream `hasBackdrop`: dims the page around the active step's target (a
    /// spotlight cutout); a press outside the callout then ends the tour with
    /// `Backdrop`.
    pub fn backdrop(mut self, backdrop: bool) -> Self {
        self.backdrop = backdrop;
        self
    }

    /// Upstream `isStepCountShown`: "2 of 5" in each step.
    pub fn step_count_shown(mut self, shown: bool) -> Self {
        self.step_count_shown = shown;
        self
    }
}

impl ParentElement for Tour {
    /// Upstream `children`: the [`TourStep`]s, and anything else.
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(elements);
    }
}

impl RenderOnce for Tour {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let key = self.id.to_string();
        let state = window.use_keyed_state(
            ElementId::Name(format!("{key}-tour").into()),
            cx,
            |window, cx| {
                // Unmounted from `on_dismiss` while focus was in the callout: that focus
                // is lost with it, so it returns (upstream's effect cleanup).
                let handle: AnyWindowHandle = window.window_handle();
                cx.on_release(move |s: &mut TourState, cx| {
                    if !s.was_running || !s.focus_in_callout {
                        return;
                    }
                    let (focused, target) = (s.focused.clone(), s.return_target.clone());
                    cx.defer(move |cx| {
                        handle
                            .update(cx, |_, window, cx| {
                                if window.focused(cx) == focused
                                    && let Some(target) = target
                                {
                                    target.focus(window, cx);
                                }
                            })
                            .ok();
                    });
                })
                .detach();
                TourState::default()
            },
        );

        let active = self.active;
        // A tour that turns inactive starts over from its first step, in this same frame
        // (upstream adjusts the index during render on the change).
        let (step_ids, index, was_active, was_running, region, return_target) =
            state.update(cx, |s, _| {
                if !active && s.was_active {
                    s.active_index = 0;
                }
                (
                    s.step_ids.clone(),
                    s.active_index,
                    s.was_active,
                    s.was_running,
                    s.region.clone(),
                    s.return_target.clone(),
                )
            });
        if active && !was_active {
            let opener = window.focused(cx);
            state.update(cx, |s, _| s.opener = opener);
        }
        let active_step = if active {
            step_ids.get(index).cloned()
        } else {
            None
        };
        // The tour ended after a step showed: focus the callout held returns.
        if active_step.is_none()
            && was_running
            && let Some(region) = region
            && region.contains_focused(window, cx)
            && let Some(target) = return_target
        {
            window.defer(cx, move |window, cx| target.focus(window, cx));
        }
        state.update(cx, |s, _| {
            s.was_active = active;
            s.was_running = active_step.is_some();
            s.focus_in_callout = false;
            if active_step.is_none() {
                s.region = None;
            }
        });

        let dismiss: DismissFn = {
            let state = state.clone();
            let user = self.on_dismiss.clone();
            Rc::new(move |source, window, cx| {
                let region = state.read(cx).region.clone();
                let in_callout = region.is_some_and(|r| r.contains_focused(window, cx));
                let focused = window.focused(cx);
                state.update(cx, |s, _| {
                    s.focus_in_callout = in_callout;
                    s.focused = focused;
                });
                user(source, window, cx);
                window.refresh();
            })
        };
        let next: StepFn = {
            let state = state.clone();
            let dismiss = dismiss.clone();
            Rc::new(move |window, cx| {
                let (index, count) = {
                    let s = state.read(cx);
                    (s.active_index, s.step_ids.len())
                };
                if index + 1 < count {
                    state.update(cx, |s, cx| {
                        s.active_index = index + 1;
                        cx.notify();
                    });
                    window.refresh();
                    return;
                }
                // Past the last step: complete.
                dismiss(TourDismissSource::Complete, window, cx);
            })
        };
        let previous: StepFn = {
            let state = state.clone();
            Rc::new(move |window, cx| {
                state.update(cx, |s, cx| {
                    s.active_index = s.active_index.saturating_sub(1);
                    cx.notify();
                });
                window.refresh();
            })
        };
        let frame = Rc::new(RefCell::new(Vec::new()));
        let context = TourContext {
            state: state.clone(),
            frame: frame.clone(),
            active_step,
            active_index: index,
            step_count: step_ids.len(),
            step_count_shown: self.step_count_shown,
            backdrop: self.backdrop,
            next,
            previous,
            dismiss,
        };
        // After the steps rendered: they register in the order they first appear, and
        // leave when they stop rendering (upstream's mount and unmount).
        let reconcile = {
            let state = state.clone();
            canvas(
                move |_, window, cx| {
                    let seen = frame.borrow().clone();
                    let changed = state.update(cx, |s, _| {
                        let mut next: Vec<ElementId> = s
                            .step_ids
                            .iter()
                            .filter(|id| seen.contains(id))
                            .cloned()
                            .collect();
                        for id in seen {
                            if !next.contains(&id) {
                                next.push(id);
                            }
                        }
                        let changed = next != s.step_ids;
                        s.step_ids = next;
                        changed
                    });
                    if changed {
                        window.request_animation_frame();
                    }
                },
                |_, _, _, _| {},
            )
            .absolute()
            .size_0()
        };
        // No chrome of its own: the children lay out as the parent's.
        provide(
            context,
            div()
                .id(self.id)
                .display_contents()
                .children(self.children)
                .child(reconcile),
        )
    }
}

/// A read-only view of the active Tour (upstream `UseTourReturn`), for custom step UIs
/// or instrumentation.
#[derive(Clone)]
pub struct UseTour {
    /// Upstream `activeStepIndex`: zero-based.
    pub active_step_index: usize,
    /// Upstream `stepCount`: the registered steps.
    pub step_count: usize,
    /// Upstream `isFirstStep`.
    pub is_first_step: bool,
    /// Upstream `isLastStep`.
    pub is_last_step: bool,
    next: StepFn,
    previous: StepFn,
    dismiss: DismissFn,
}

impl UseTour {
    /// Upstream `next`: advance, or complete on the last step.
    pub fn next(&self, window: &mut Window, cx: &mut App) {
        (self.next)(window, cx)
    }

    /// Upstream `previous`: back one step (none from the first).
    pub fn previous(&self, window: &mut Window, cx: &mut App) {
        (self.previous)(window, cx)
    }

    /// Upstream `dismiss`: end the tour with a reason.
    pub fn dismiss(&self, source: TourDismissSource, window: &mut Window, cx: &mut App) {
        (self.dismiss)(source, window, cx)
    }
}

/// Upstream `useTour`: the nearest Tour's state, read during render; `None` outside a
/// Tour.
pub fn use_tour(cx: &App) -> Option<UseTour> {
    let tour = use_context::<TourContext>(cx)?;
    Some(UseTour {
        active_step_index: tour.active_index,
        step_count: tour.step_count,
        is_first_step: tour.active_index == 0,
        is_last_step: tour.step_count > 0 && tour.active_index + 1 == tour.step_count,
        next: tour.next,
        previous: tour.previous,
        dismiss: tour.dismiss,
    })
}

const fn p(prop: &'static str, rust: RustApi) -> PropMapping {
    PropMapping { prop, rust }
}

/// Upstream lab `Tour` and its `TourStep` (whose props are prefixed `TourStep.`).
pub const TOUR_DOC: ComponentDoc = ComponentDoc {
    name: "Tour",
    rust_type: "rustryx::lab::tour::Tour (with TourStep, TourTarget and use_tour)",
    props: &[
        p("isActive", RustApi::Argument("active")),
        p("children", RustApi::Trait("ParentElement")),
        p("onDismiss", RustApi::Argument("on_dismiss")),
        p("hasBackdrop", RustApi::Method("backdrop")),
        p("isStepCountShown", RustApi::Method("step_count_shown")),
        p(
            "TourStep.targetRef",
            RustApi::Argument("target (a TourTarget that wraps the element)"),
        ),
        p("TourStep.heading", RustApi::Argument("heading")),
        p("TourStep.children", RustApi::Trait("ParentElement")),
        p("TourStep.placement", RustApi::Method("placement")),
        p("TourStep.alignment", RustApi::Method("alignment")),
        p("TourStep.data-testid", RustApi::Method("test_id")),
    ],
    example: include_str!("examples.rs"),
};
