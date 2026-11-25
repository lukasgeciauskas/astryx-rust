//! BottomSheet and BottomSheetSwitcher, upstream `BottomSheet/`: a touch
//! sheet that rises from the bottom edge.
//!
//! - Up to 640px wide, centered, with rounded top corners, a border and a
//!   high shadow; it slides up over `--duration-medium` and back down on the
//!   exit easing.
//! - Heights: `Hug` fits its content up to 92% of the viewport, `Capped` is
//!   62%, `Tall` 92%, or any length or viewport fraction. Its body scrolls,
//!   padded by `spacing-4` unless `padding` says otherwise, and keeps a band
//!   clear under the grab handle.
//! - The grab handle drags the sheet (mouse or finger): released, it rests at
//!   the nearest of its snap points (its own height is the tallest), and a
//!   pull well below the shortest dismisses it when `purpose` allows a swipe
//!   (`Info`).
//! - With a scrim it is modal, like a Dialog (focus region, entry and
//!   return; Escape and scrim presses per `purpose`); without one the page
//!   stays interactive.

use std::rc::Rc;
use std::time::Duration;

use astryx_primitives::{prefers_reduced_motion, provide};
use astryx_theme::{ThemeStyled, current};
use gpui::{
    AnyElement, App, DispatchPhase, ElementId, IntoElement, MouseButton, MouseMoveEvent,
    MouseUpEvent, ParentElement, Pixels, RenderOnce, Role, SharedString, StyleRefinement, Styled,
    Window, div, prelude::*, px, relative,
};
use smallvec::SmallVec;

use super::dialog::{DialogPurpose, inset};
use super::modal::{
    ModalState, Phase, backdrop, faded, modal_layer, modal_state, phase, probes, sync_open,
};
use crate::layout::SpacingStep;

type OpenFn = Rc<dyn Fn(bool, &mut Window, &mut App)>;

/// The open modal sheets in open order (BottomSheet FR5): the last is on
/// top; the others are covered.
#[derive(Default)]
struct SheetStack(Vec<String>);

impl gpui::Global for SheetStack {}

/// How many open modal sheets cover the sheet `key` (0 on top), after
/// noting whether it is open.
fn stack_depth(key: &str, open: bool, cx: &mut App) -> usize {
    let stack = cx.default_global::<SheetStack>();
    let at = stack.0.iter().position(|k| k == key);
    match (open, at) {
        (true, None) => {
            stack.0.push(key.to_string());
            0
        }
        (true, Some(i)) => stack.0.len() - 1 - i,
        (false, Some(i)) => {
            stack.0.remove(i);
            0
        }
        (false, None) => 0,
    }
}

/// Upstream `height`.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum SheetHeight {
    /// Fits its content, up to 92% of the viewport.
    Hug,
    /// 62% of the viewport, scrolling.
    #[default]
    Capped,
    /// 92% of the viewport.
    Tall,
    Length(Pixels),
    /// A fraction of the viewport's height.
    Fraction(f32),
}

/// One of upstream's `snapPoints`: an extra height the sheet can rest at.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SnapPoint {
    /// A fraction of the viewport, above 0 and up to 1.
    Fraction(f32),
    Length(Pixels),
}

/// The sheet is this far below its shortest stop, relative to that stop,
/// when a release dismisses it (upstream `DISMISS_OVERSHOOT_RATIO`).
const DISMISS_OVERSHOOT: f32 = 0.4;

/// A sheet rising from the bottom edge, upstream `BottomSheet`.
#[derive(IntoElement)]
pub struct BottomSheet {
    id: ElementId,
    open: bool,
    label: SharedString,
    on_open_change: Option<OpenFn>,
    children: SmallVec<[AnyElement; 2]>,
    purpose: DialogPurpose,
    padding: Option<SpacingStep>,
    height: SheetHeight,
    snap_points: Vec<SnapPoint>,
    scrim: bool,
    sheet_id: Option<SharedString>,
    style: StyleRefinement,
}

impl BottomSheet {
    /// Upstream `isOpen` and `label`, the sheet's accessible name.
    pub fn new(id: impl Into<ElementId>, open: bool, label: impl Into<SharedString>) -> Self {
        BottomSheet {
            id: id.into(),
            open,
            label: label.into(),
            on_open_change: None,
            children: SmallVec::new(),
            purpose: DialogPurpose::Info,
            padding: None,
            height: SheetHeight::Capped,
            snap_points: Vec::new(),
            scrim: true,
            sheet_id: None,
            style: StyleRefinement::default(),
        }
    }

    astryx_primitives::setters! {
        /// Upstream `purpose`: `Info` also allows a swipe down to dismiss.
        purpose: DialogPurpose,
        /// Upstream `padding`: `spacing-4` by default.
        ?padding: SpacingStep,
        /// Upstream `height`.
        height: SheetHeight,
        /// Upstream `hasScrim`.
        scrim: bool,
        /// Upstream `sheetId`, for a BottomSheetSwitcher.
        ?sheet_id: SharedString,
    }

    /// Upstream `snapPoints`.
    pub fn snap_points(mut self, points: impl IntoIterator<Item = SnapPoint>) -> Self {
        self.snap_points = points.into_iter().collect();
        self
    }

    /// Upstream `onOpenChange`.
    pub fn on_open_change(mut self, f: impl Fn(bool, &mut Window, &mut App) + 'static) -> Self {
        self.on_open_change = Some(Rc::new(f));
        self
    }

    pub(crate) fn opened(mut self, open: bool) -> Self {
        self.open = open;
        self
    }
}

impl ParentElement for BottomSheet {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(elements);
    }
}

impl Styled for BottomSheet {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

/// Where a released drag leaves the sheet: the height to rest at, or `None`
/// to dismiss it.
pub(crate) fn settle(
    own: Pixels,
    stops: &[Pixels],
    dragged: Pixels,
    swipe: bool,
) -> Option<Pixels> {
    let visible = own - dragged;
    let shortest = stops.iter().copied().fold(own, Pixels::min);
    if swipe && visible < shortest - shortest * DISMISS_OVERSHOOT {
        return None;
    }
    let nearest = stops
        .iter()
        .copied()
        .chain([own])
        .min_by(|a, b| {
            f32::from((*a - visible).abs())
                .partial_cmp(&f32::from((*b - visible).abs()))
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .unwrap_or(own);
    Some(nearest)
}

impl RenderOnce for BottomSheet {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = current(window, cx);
        let t = theme.tokens();
        let rem = window.rem_size();
        let state = modal_state(&self.id, window, cx);
        sync_open(&state, self.open, window, cx);
        let reduced = prefers_reduced_motion(cx);
        let duration = if reduced {
            Duration::from_millis(10)
        } else {
            t.duration.medium
        };
        let shown = match phase(&state, duration, duration, window, cx) {
            Phase::Hidden => {
                return div()
                    .id(self.id.clone())
                    .display_contents()
                    .into_any_element();
            }
            Phase::Open { progress } => t.ease.standard.ease(progress),
            Phase::Closing { progress } => 1. - t.ease.exit.ease(progress),
        };

        // BottomSheet FR5, FR6: a covered sheet recedes a step behind the top
        // one and leaves the stack's one backdrop to it.
        let key = format!("{:?}", self.id);
        let depth = if self.scrim {
            stack_depth(&key, state.read(cx).open, cx)
        } else {
            0
        };
        let viewport = window.viewport_size();
        let budget = |height: SheetHeight| match height {
            SheetHeight::Hug | SheetHeight::Tall => viewport.height * 0.92,
            SheetHeight::Capped => viewport.height * 0.62,
            SheetHeight::Length(px) => px.min(viewport.height),
            SheetHeight::Fraction(f) => viewport.height * f.clamp(0., 1.),
        };
        let own = budget(self.height);
        let stops: Vec<Pixels> = self
            .snap_points
            .iter()
            .map(|p| match p {
                SnapPoint::Fraction(f) => viewport.height * f.clamp(0., 1.),
                SnapPoint::Length(px) => *px,
            })
            .filter(|h| *h > px(0.) && *h < own)
            .collect();
        let (drag, rest) = {
            let s = state.read(cx);
            (s.drag.map(|(_, by)| by), s.rest)
        };

        let padding = self
            .padding
            .map_or(t.spacing.s4.to_pixels(rem), |s| s.token(t).to_pixels(rem));
        let handle_band = t.spacing.s6.to_pixels(rem);
        let report = self.on_open_change.clone();
        let close = move |window: &mut Window, cx: &mut App| {
            if let Some(f) = &report {
                f(false, window, cx);
            }
        };

        // The handle: a pill on a band that fades the content under it.
        let drag_start = state.clone();
        let handle = div()
            .id(ElementId::Name(format!("{}-handle", self.id).into()))
            .absolute()
            .top_0()
            .left_0()
            .right_0()
            .h(handle_band)
            .flex()
            .items_center()
            .justify_center()
            .cursor_grab()
            .on_mouse_down(MouseButton::Left, move |event, window, cx| {
                cx.stop_propagation();
                drag_start.update(cx, |s, _| s.drag = Some((event.position.y, px(0.))));
                window.refresh();
            })
            .child(
                div()
                    .w(t.size.element_lg)
                    .h(t.spacing.s1)
                    .rounded_full()
                    .bg(t.color.border),
            );

        let surface_focus = state.read(cx).surface.clone();
        let body = div()
            .id(ElementId::Name(format!("{}-body", self.id).into()))
            .flex_grow(1.)
            .min_h_0()
            .overflow_y_scroll()
            .pt(padding.max(handle_band))
            .px(padding)
            .pb(padding)
            .child(provide(
                state.read(cx).focus.clone(),
                provide(
                    inset(padding),
                    crate::layout::host_children(
                        self.children,
                        crate::layout::ContainerPadding {
                            inline_start: padding,
                            inline_end: padding,
                            block_start: padding.max(handle_band),
                            block_end: padding,
                        },
                        crate::layout::ContainerCorners {
                            start_start: t.radius.container.to_pixels(rem),
                            start_end: t.radius.container.to_pixels(rem),
                            ..Default::default()
                        },
                    ),
                ),
            ));
        let mut sheet = div()
            .id(ElementId::Name(format!("{}-sheet", self.id).into()))
            .track_focus(&surface_focus)
            .occlude()
            .relative()
            .flex()
            .flex_col()
            .w_full()
            .max_w(px(640.))
            .bg(t.color.background_surface)
            .text_color(t.color.text_primary)
            .font_stack(&t.font.family.body)
            .text_style(&t.text.body)
            .border_t_1()
            .border_l_1()
            .border_r_1()
            .border_color(t.color.border)
            .rounded_t(t.radius.container)
            .shadow_token(&t.shadow.high)
            .role(Role::Dialog)
            .aria_label(self.label.clone())
            .aria_modal(self.scrim)
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation());
        sheet = match (self.height, rest) {
            (_, Some(rest)) => sheet.h(rest),
            (SheetHeight::Hug, None) => sheet.max_h(own),
            (_, None) => sheet.h(own),
        };
        if self.scrim {
            sheet = sheet.tab_trap();
        }
        // Rising in, and following a drag.
        let dragged = drag.unwrap_or(px(0.)).max(px(0.));
        sheet = if reduced {
            sheet.opacity(shown).translate(px(0.), dragged)
        } else {
            sheet.translate(px(0.), relative(1. - shown)).mt(dragged)
        };
        if depth > 0 && !reduced {
            let steps = depth.min(2) as f32;
            sheet = sheet
                .scale(1. - 0.04 * steps)
                .mt(-(t.spacing.s2.to_pixels(rem) * steps));
        }
        theme.refine(sheet.style(), "bottom-sheet", &[], None);
        sheet.style().refine(&self.style);
        let mut sheet = sheet.children(probes(&state, cx)).child(body).child(handle);

        // While dragging: follow the pointer anywhere, settle on release.
        if drag.is_some() {
            let swipe = self.purpose == DialogPurpose::Info;
            let resting = rest.unwrap_or(own);
            let (moving, releasing) = (state.clone(), state.clone());
            let release_close = close.clone();
            sheet = sheet.child(
                gpui::canvas(
                    |_, _, _| {},
                    move |_, _, window, _| {
                        let moving = moving.clone();
                        window.on_mouse_event(move |event: &MouseMoveEvent, phase, window, cx| {
                            if phase != DispatchPhase::Capture {
                                return;
                            }
                            moving.update(cx, |s: &mut ModalState, _| {
                                if let Some((from, by)) = s.drag.as_mut() {
                                    *by = event.position.y - *from;
                                }
                            });
                            window.refresh();
                        });
                        let releasing = releasing.clone();
                        let stops = stops.clone();
                        let close = release_close.clone();
                        window.on_mouse_event(move |_: &MouseUpEvent, phase, window, cx| {
                            if phase != DispatchPhase::Capture {
                                return;
                            }
                            let dragged = releasing
                                .update(cx, |s: &mut ModalState, _| s.drag.take())
                                .map_or(px(0.), |(_, by)| by);
                            match settle(resting, &stops, dragged, swipe) {
                                Some(height) => {
                                    releasing.update(cx, |s, _| s.rest = Some(height));
                                }
                                None => close(window, cx),
                            }
                            window.refresh();
                        });
                    },
                )
                .absolute()
                .size_0(),
            );
        }

        let info = self.purpose == DialogPurpose::Info;
        let press: super::modal::CloseFn = Rc::new(close.clone());
        let holder = if self.scrim {
            backdrop(
                &self.id,
                // One dim for the stack: a covered sheet's backdrop is clear.
                Some(faded(t.color.overlay, if depth > 0 { 0. } else { shown })),
                info.then_some(press),
                window,
            )
            .flex()
            .justify_center()
            .items_end()
            .child(sheet)
            .into_any_element()
        } else {
            // No scrim: a positioner that takes no presses of its own.
            div()
                .absolute()
                .top_0()
                .left_0()
                .w(viewport.width)
                .h(viewport.height)
                .flex()
                .justify_center()
                .items_end()
                .child(sheet)
                .into_any_element()
        };
        modal_layer(&self.id, holder, self.scrim, self.purpose, close)
    }
}

type ActiveFn = Rc<dyn Fn(Option<SharedString>, &mut Window, &mut App)>;

/// Steps through sheets, one open at a time, upstream `BottomSheetSwitcher`
/// (deprecated upstream: prefer one BottomSheet per step, opened by one
/// state value).
#[derive(IntoElement)]
pub struct BottomSheetSwitcher {
    active_sheet: Option<SharedString>,
    on_active_sheet_change: Option<ActiveFn>,
    scrim: bool,
    sheets: Vec<BottomSheet>,
}

impl BottomSheetSwitcher {
    /// Upstream `activeSheet`: the open sheet's id, or `None`.
    pub fn new(active_sheet: Option<SharedString>) -> Self {
        BottomSheetSwitcher {
            active_sheet,
            on_active_sheet_change: None,
            scrim: true,
            sheets: Vec::new(),
        }
    }

    /// Upstream `onActiveSheetChange`: `None` when the active sheet
    /// dismisses.
    pub fn on_active_sheet_change(
        mut self,
        f: impl Fn(Option<SharedString>, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_active_sheet_change = Some(Rc::new(f));
        self
    }

    /// Upstream `hasScrim`.
    pub fn scrim(mut self, scrim: bool) -> Self {
        self.scrim = scrim;
        self
    }

    /// Upstream `children`: a sheet with its `sheet_id`.
    pub fn sheet(mut self, sheet: BottomSheet) -> Self {
        self.sheets.push(sheet);
        self
    }
}

impl RenderOnce for BottomSheetSwitcher {
    fn render(self, _: &mut Window, _: &mut App) -> impl IntoElement {
        let active = self.active_sheet;
        let report = self.on_active_sheet_change;
        div()
            .display_contents()
            .children(self.sheets.into_iter().map(|sheet| {
                let open = sheet.sheet_id.is_some() && sheet.sheet_id == active;
                let report = report.clone();
                sheet
                    .opened(open)
                    .scrim(self.scrim)
                    .on_open_change(move |open, window, cx| {
                        if !open && let Some(f) = &report {
                            f(None, window, cx);
                        }
                    })
            }))
    }
}
