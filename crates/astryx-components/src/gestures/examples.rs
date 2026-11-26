//! The gesture hooks' playing-cards example (spec:AST-107 FR5), built only on the public
//! layer: a card that flicks and snaps back, a swipe that discards a card, a card that
//! flips on a long press, a card menu on the menu press model, and a local card that
//! paints its press as Astryx's own controls do.

use gpui::{
    AnyElement, App, ElementId, IntoElement, ParentElement, RenderOnce, SharedString, Styled,
    Window, div, prelude::*, px,
};

use super::menu_press::{MenuPressOptions, MenuPressRow, use_menu_press};
use super::{
    LongPressOptions, PanAxis, PanGestureOptions, PanState, after_gesture, use_long_press,
    use_pan_gesture, use_press_feedback,
};
use crate::animator::use_animator;
use crate::button::Button;
use crate::layout::{SpacingStep, Stack};
use crate::text::{Text, TextType};
use astryx_primitives::motion::{AnimatorAnimateOptions, AnimatorValues};
use astryx_theme::ThemeStyled as _;

pub fn examples(_window: &mut Window, _cx: &mut App) -> AnyElement {
    let label = |text: &'static str| Text::new(text).text_type(TextType::Supporting);
    Stack::vertical()
        .gap(SpacingStep::S3)
        .child(label("Playing cards: the gesture hooks"))
        .child(
            div()
                .flex()
                .flex_wrap()
                .gap(px(24.))
                .child(FlickCard {
                    id: "cards-flick".into(),
                })
                .child(SwipeCard {
                    id: "cards-swipe".into(),
                })
                .child(FlipCard {
                    id: "cards-flip".into(),
                })
                .child(CardMenu {
                    id: "cards-menu".into(),
                })
                .child(PressCard {
                    id: "cards-press".into(),
                })
                .child(DeckMenu {
                    id: "cards-deck".into(),
                }),
        )
        .into_any_element()
}

fn card(
    id: impl Into<ElementId>,
    face: impl Into<SharedString>,
    window: &mut Window,
    cx: &mut App,
) -> gpui::Stateful<gpui::Div> {
    let theme = astryx_theme::current(window, cx);
    let t = theme.tokens();
    div()
        .id(id.into())
        .w(px(96.))
        .h(px(136.))
        .flex()
        .items_center()
        .justify_center()
        .rounded(t.radius.container)
        .border_1()
        .border_color(t.color.border)
        .bg(t.color.background_surface)
        .text_color(t.color.text_primary)
        .text_size(px(28.))
        .child(face.into())
}

/// usePanGesture with useAnimator: the card follows the finger from the claim and springs
/// home from the release velocity.
#[derive(IntoElement)]
struct FlickCard {
    id: ElementId,
}

impl RenderOnce for FlickCard {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let animator = use_animator(
            ElementId::Name(format!("{}-card", self.id).into()),
            window,
            cx,
        );
        let follow = animator.clone();
        let pan = use_pan_gesture(
            self.id.clone(),
            PanGestureOptions::new(move |e| {
                let (follow, e) = (follow.clone(), *e);
                after_gesture(move |_, cx| match e.state {
                    PanState::Began | PanState::Changed => follow.set(
                        AnimatorValues::new().x(e.translation.x).y(e.translation.y),
                        cx,
                    ),
                    PanState::Ended | PanState::Cancelled => {
                        follow.animate(
                            AnimatorValues::new().x(0.).y(0.),
                            AnimatorAnimateOptions {
                                velocity: AnimatorValues::new().x(e.velocity.x).y(e.velocity.y),
                            },
                            cx,
                        );
                    }
                });
            }),
            window,
            cx,
        );
        Stack::vertical()
            .gap(SpacingStep::S1)
            .child(pan.area(animator.apply(card(self.id.clone(), "A♠", window, cx), window, cx)))
            .child(Text::new("Flick me").text_type(TextType::Supporting))
    }
}

/// A horizontal pan that discards the card past a third of its travel or on a fling.
#[derive(IntoElement)]
struct SwipeCard {
    id: ElementId,
}

impl RenderOnce for SwipeCard {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let animator = use_animator(
            ElementId::Name(format!("{}-card", self.id).into()),
            window,
            cx,
        );
        let follow = animator.clone();
        let pan = use_pan_gesture(
            self.id.clone(),
            PanGestureOptions::new(move |e| {
                let (follow, e) = (follow.clone(), *e);
                after_gesture(move |_, cx| match e.state {
                    PanState::Began | PanState::Changed => {
                        follow.set(AnimatorValues::new().x(e.translation.x), cx)
                    }
                    PanState::Ended
                        if e.translation.x.abs() > 120. || e.velocity.x.abs() > 1000. =>
                    {
                        let away = 480. * e.translation.x.signum();
                        follow.animate(
                            AnimatorValues::new().x(away).opacity(0.),
                            AnimatorAnimateOptions {
                                velocity: AnimatorValues::new().x(e.velocity.x),
                            },
                            cx,
                        );
                    }
                    _ => {
                        follow.animate(AnimatorValues::new().x(0.), Default::default(), cx);
                    }
                });
            })
            .axis(PanAxis::X),
            window,
            cx,
        );
        let deal = animator.clone();
        Stack::vertical()
            .gap(SpacingStep::S1)
            .child(pan.area(animator.apply(card(self.id.clone(), "K♥", window, cx), window, cx)))
            .child(
                Button::new(
                    ElementId::Name(format!("{}-deal", self.id).into()),
                    "Deal again",
                )
                .on_click(move |_, _, cx| deal.set(AnimatorValues::new().x(0.).opacity(1.), cx)),
            )
    }
}

/// useLongPress: a held finger flips the card (the keyboard path is the button).
#[derive(IntoElement)]
struct FlipCard {
    id: ElementId,
}

impl RenderOnce for FlipCard {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let up = window.use_keyed_state(
            ElementId::Name(format!("{}-up", self.id).into()),
            cx,
            |_, _| true,
        );
        let flip = up.clone();
        let hold = use_long_press(
            self.id.clone(),
            LongPressOptions::new(move |_, window, cx| {
                flip.update(cx, |u, _| *u = !*u);
                window.refresh();
            }),
            window,
            cx,
        );
        let face = if *up.read(cx) { "Q♦" } else { "🂠" };
        let button = up.clone();
        Stack::vertical()
            .gap(SpacingStep::S1)
            .child(hold.area(card(self.id.clone(), face, window, cx)))
            .child(
                Button::new(ElementId::Name(format!("{}-flip", self.id).into()), "Flip").on_click(
                    move |_, window, cx| {
                        button.update(cx, |u, _| *u = !*u);
                        window.refresh();
                    },
                ),
            )
    }
}

/// useMenuPress: a mouse presses the trigger and drags onto a row; a finger opens the menu
/// as it presses and picks after the handoff; the release acts on the row under it.
#[derive(IntoElement)]
struct CardMenu {
    id: ElementId,
}

impl RenderOnce for CardMenu {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = astryx_theme::current(window, cx);
        let t = theme.tokens().clone();
        let state = window.use_keyed_state(
            ElementId::Name(format!("{}-state", self.id).into()),
            cx,
            |_, _| {
                (
                    false,
                    Option::<SharedString>::None,
                    SharedString::from("J♣"),
                )
            },
        );
        let (open, lit, face) = state.read(cx).clone();
        let (o, h, d) = (state.clone(), state.clone(), state.clone());
        let press = use_menu_press(
            self.id.clone(),
            MenuPressOptions::default()
                .on_trigger_press(move |_, window, cx| {
                    let was = o.read(cx).0;
                    o.update(cx, |s, _| s.0 = !was);
                    window.refresh();
                    !was
                })
                .on_highlight(move |row, window, cx| {
                    h.update(cx, |s, _| s.1 = row);
                    window.refresh();
                })
                .on_dismiss(move |window, cx| {
                    d.update(cx, |s, _| s.0 = false);
                    window.refresh();
                }),
            window,
            cx,
        );
        let toggle = state.clone();
        let trigger = press.trigger(
            Button::new(
                ElementId::Name(format!("{}-trigger", self.id).into()),
                "Change suit",
            )
            .on_click({
                let press = press.clone();
                move |_, window, cx| {
                    if press.is_trigger_click_from_press() {
                        return;
                    }
                    toggle.update(cx, |s, _| s.0 = !s.0);
                    window.refresh();
                }
            }),
        );
        let rows = ["J♣", "J♦", "J♥", "J♠"].map(|suit| {
            let pick = state.clone();
            let lit_now = lit.as_deref() == Some(suit);
            press.row(
                MenuPressRow::new(suit).on_activate(move |_, window, cx| {
                    pick.update(cx, |s, _| {
                        s.2 = suit.into();
                        s.0 = false;
                    });
                    window.refresh();
                }),
                div()
                    .id(ElementId::Name(format!("{suit}-row").into()))
                    .px(px(12.))
                    .py(px(6.))
                    .rounded(t.radius.inner)
                    .when(lit_now, |el| el.bg(t.color.overlay_hover))
                    .child(suit),
            )
        });
        let menu = open.then(|| {
            press.menu(
                div()
                    .flex()
                    .flex_col()
                    .p(px(4.))
                    .rounded(t.radius.container)
                    .bg(t.color.background_popover)
                    .shadow_token(&t.shadow.low)
                    .children(rows),
                true,
            )
        });
        Stack::vertical()
            .gap(SpacingStep::S1)
            .child(card(self.id.clone(), face, window, cx))
            .child(trigger)
            .children(menu)
    }
}

/// usePressFeedback: a local card that paints its press as Astryx's controls do, at once
/// outside a scroller and after the content-touch delay inside one, fading on release.
#[derive(IntoElement)]
struct PressCard {
    id: ElementId,
}

impl RenderOnce for PressCard {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = astryx_theme::current(window, cx);
        let pressed = theme.tokens().color.overlay_pressed;
        let feedback = use_press_feedback(self.id.clone(), window, cx);
        let mut el = card(self.id.clone(), "10♠", window, cx).cursor_pointer();
        if let Some(overlay) = feedback.overlay(pressed, window, cx) {
            el = el.bg(astryx_theme::color::composite(
                theme.tokens().color.background_surface,
                overlay,
            ));
        } else if !feedback.paints_buttons(window, cx) {
            el = el.active(move |s| s.bg(pressed));
        }
        Stack::vertical()
            .gap(SpacingStep::S1)
            .child(feedback.surface(el))
            .child(Text::new("Press me").text_type(TextType::Supporting))
    }
}

/// createContextMenu / useContextMenu: a right press, a long press or Shift+F10 on the
/// card opens its menu, with no wrapper around it.
#[derive(IntoElement)]
struct DeckMenu {
    id: ElementId,
}

impl RenderOnce for DeckMenu {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let shuffled = window.use_keyed_state(
            ElementId::Name(format!("{}-shuffled", self.id).into()),
            cx,
            |_, _| 0usize,
        );
        let s = shuffled.clone();
        let menu = crate::overlay::use_context_menu(
            self.id.clone(),
            crate::overlay::ContextMenuOptions::default()
                .label("Deck")
                .items([
                    crate::overlay::MenuItem::new("shuffle", "Shuffle")
                        .on_click(move |window, cx| {
                            s.update(cx, |n, _| *n += 1);
                            window.refresh();
                        })
                        .into(),
                    crate::overlay::MenuItem::new("deal", "Deal").into(),
                ]),
            window,
            cx,
        );
        let times = *shuffled.read(cx);
        Stack::vertical()
            .gap(SpacingStep::S1)
            .child(menu.attach(
                card(self.id.clone(), "🂡", window, cx).relative(),
                window,
                cx,
            ))
            .child(Text::new(format!("Shuffled {times} times")).text_type(TextType::Supporting))
    }
}
