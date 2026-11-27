//! The snap-point drag's playing-card examples (spec:AST-107 FR5), upstream's CLI
//! templates `useDragToSnapPoints/DragToSnapPointsCard` and
//! `createDragToSnapPoints/DragToSnapPointsCardVanilla`: a card that a drag
//! sideways settles on one of three places (the nearest, or the next after a quick
//! flick), with a button for each place (the keyboard path) and an announcement of
//! where it landed. The places run in reading order, so leftward right to left.

use gpui::{
    AnyElement, App, ElementId, IntoElement, ParentElement, RenderOnce, SharedString, Styled,
    Window, div, prelude::*, px,
};
use rustryx_primitives::gestures::host::gesture_host;
use rustryx_primitives::{Politeness, announce};

use super::{
    DragToSnapPoints, DragToSnapPointsOptions, PanAxis, SnapElement, create_drag_to_snap_points,
    use_drag_to_snap_points,
};
use crate::button::{Button, ButtonVariant};
use crate::layout::{SpacingStep, Stack};
use crate::text::{Text, TextType};
use rustryx_theme::ThemeStyled;

/// The places, in reading order.
const PLACES: [&str; 3] = ["Hand", "Table", "Discard pile"];
/// How far along the row each place sits, px from where the card rests.
const STEPS: [f64; 3] = [0., 110., 220.];

pub fn examples(window: &mut Window, cx: &mut App) -> AnyElement {
    let label = |text: &'static str| Text::new(text).text_type(TextType::Supporting);
    let _ = (window, cx);
    Stack::vertical()
        .gap(SpacingStep::S3)
        .child(label(
            "useDragToSnapPoints: drag the card sideways and let go; it settles on the nearest place, or the next one after a quick flick",
        ))
        .child(DragToSnapPointsCard {
            id: "snap-card".into(),
        })
        .child(label(
            "createDragToSnapPoints: the same card on the factory, without the hook",
        ))
        .child(DragToSnapPointsCardFactory {
            id: "snap-card-factory".into(),
        })
        .into_any_element()
}

/// The snap points in reading order: negative right to left, so they sort the other
/// way round, and the place an index names.
fn places(rtl: bool) -> (Vec<f64>, impl Fn(usize) -> usize + Clone) {
    let points = if rtl {
        STEPS.iter().map(|s| -s).collect()
    } else {
        STEPS.to_vec()
    };
    (
        points,
        move |index: usize| {
            if rtl { STEPS.len() - 1 - index } else { index }
        },
    )
}

#[cfg(test)]
pub(crate) fn places_for_test(rtl: bool) -> (Vec<f64>, impl Fn(usize) -> usize + Clone) {
    places(rtl)
}

fn announcement(place: usize) -> SharedString {
    format!("The card is on the {}.", PLACES[place].to_lowercase()).into()
}

/// The card: the theme's surface and text, so it reads in light and dark.
fn card(window: &mut Window, cx: &mut App, label: SharedString) -> gpui::Stateful<gpui::Div> {
    let theme = rustryx_theme::current(window, cx);
    let t = theme.tokens();
    let rem = window.rem_size();
    div()
        .id(label.clone())
        .w(px(96.))
        .h(px(136.))
        .flex()
        .flex_col()
        .items_center()
        .justify_center()
        .gap(t.spacing.s1.to_pixels(rem))
        .rounded(t.radius.container)
        .bg(t.color.background_surface)
        .text_color(t.color.text_primary)
        .border_1()
        .border_color(t.color.border)
        .shadow_token(&t.shadow.low)
        .cursor_grab()
        .font_stack(&t.font.family.body)
        .child(Text::new("Q").text_type(TextType::Display2))
        .child(Text::new("\u{2665}").text_type(TextType::Display3))
        .role(gpui::Role::Group)
        .aria_label(label)
}

#[derive(IntoElement)]
struct DragToSnapPointsCard {
    id: ElementId,
}

impl RenderOnce for DragToSnapPointsCard {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let place = window.use_keyed_state(
            ElementId::Name(format!("{}-place", self.id).into()),
            cx,
            |_, _| 0usize,
        );
        let shown = *place.read(cx);
        let rtl = rustryx_i18n::use_direction(cx).is_rtl();
        let (points, place_at) = places(rtl);
        let settled = place.clone();
        let at = place_at.clone();
        let snap = use_drag_to_snap_points(
            ElementId::Name(format!("{}-card", self.id).into()),
            DragToSnapPointsOptions::new(PanAxis::X, points, move |index, window, cx| {
                let place = at(index);
                settled.update(cx, |p, _| *p = place);
                announce(announcement(place), Politeness::Polite, window, cx);
                window.refresh();
            }),
            window,
            cx,
        );
        let label: SharedString =
            format!("Queen of hearts, on the {}", PLACES[shown].to_lowercase()).into();
        let card = card(window, cx, label);
        let buttons = PLACES.iter().enumerate().map(|(index, name)| {
            let (snap, at) = (snap.clone(), place_at.clone());
            // Each place's index along the snap points.
            let target = (0..PLACES.len()).find(|i| at(*i) == index).unwrap_or(index);
            Button::new(
                ElementId::Name(format!("{}-{index}", self.id).into()),
                *name,
            )
            .variant(if index == shown {
                ButtonVariant::Primary
            } else {
                ButtonVariant::Secondary
            })
            .on_click(move |_, _, cx| snap.settle_to(target, cx))
        });
        Stack::vertical()
            .gap(SpacingStep::S4)
            .max_width(px(380.))
            .child(
                div()
                    .h(px(150.))
                    .flex()
                    .items_center()
                    .child(snap.area(card, window, cx)),
            )
            .child(Stack::horizontal().gap(SpacingStep::S2).children(buttons))
    }
}

/// The factory version: the drag is made once and taken down with the example.
#[derive(IntoElement)]
struct DragToSnapPointsCardFactory {
    id: ElementId,
}

struct FactoryState {
    drag: DragToSnapPoints,
    rtl: bool,
}

impl Drop for FactoryState {
    fn drop(&mut self) {
        self.drag.destroy();
    }
}

impl RenderOnce for DragToSnapPointsCardFactory {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let rtl = rustryx_i18n::use_direction(cx).is_rtl();
        let host = gesture_host(window, cx);
        let element = window
            .use_keyed_state(
                ElementId::Name(format!("{}-element", self.id).into()),
                cx,
                |_, _| SnapElement::new(),
            )
            .read(cx)
            .clone();
        let make = {
            let (host, element) = (host.clone(), element.clone());
            move |rtl: bool, cx: &mut App| {
                let (points, place_at) = places(rtl);
                create_drag_to_snap_points(
                    &element,
                    &host,
                    DragToSnapPointsOptions::new(PanAxis::X, points, move |index, window, cx| {
                        announce(
                            announcement(place_at(index)),
                            Politeness::Polite,
                            window,
                            cx,
                        );
                    }),
                    cx,
                )
            }
        };
        let state =
            window.use_keyed_state(ElementId::Name(format!("{}-drag", self.id).into()), cx, {
                let make = make.clone();
                move |_, cx| FactoryState {
                    drag: make(rtl, cx),
                    rtl,
                }
            });
        if state.read(cx).rtl != rtl {
            state.update(cx, |s, cx| {
                s.drag.destroy();
                s.drag = make(rtl, cx);
                s.rtl = rtl;
            });
        }
        let drag = state.read(cx).drag.clone();
        let (_, place_at) = places(rtl);
        let card = card(window, cx, "Q\u{2665}".into());
        let buttons = PLACES.iter().enumerate().map(|(index, name)| {
            let drag = drag.clone();
            let target = (0..PLACES.len())
                .find(|i| place_at(*i) == index)
                .unwrap_or(index);
            Button::new(
                ElementId::Name(format!("{}-{index}", self.id).into()),
                *name,
            )
            .variant(ButtonVariant::Secondary)
            .on_click(move |_, _, cx| drag.settle_to(target, cx))
        });
        Stack::vertical()
            .gap(SpacingStep::S4)
            .max_width(px(380.))
            .child(
                div()
                    .h(px(150.))
                    .flex()
                    .items_center()
                    .child(drag.area(card, window, cx)),
            )
            .child(Stack::horizontal().gap(SpacingStep::S2).children(buttons))
    }
}
