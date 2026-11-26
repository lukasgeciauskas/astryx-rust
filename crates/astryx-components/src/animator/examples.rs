//! Animator examples for the gallery.

use gpui::{
    AnyElement, App, ElementId, IntoElement, ParentElement, RenderOnce, Styled, Window, div, px,
};

use super::{EntryAnimationPreset, use_animator, use_entry_animation};
use crate::button::Button;
use crate::layout::{SpacingStep, Stack};
use crate::text::{Text, TextType};
use astryx_primitives::motion::AnimatorValues;

pub fn examples(window: &mut Window, cx: &mut App) -> AnyElement {
    let label = |text: &'static str| Text::new(text).text_type(TextType::Supporting);
    let animated_layout = crate::animated_layout::examples::examples(window, cx);
    Stack::vertical()
        .gap(SpacingStep::S3)
        .child(label("useAnimator: springs a card on the state-change role; press again mid-flight and it carries its velocity"))
        .child(SpringDemo { id: "animator-demo".into() })
        .child(label("useEntryAnimation: each added row slides in; the rows drawn on first paint did not"))
        .child(EntryDemo { id: "entry-demo".into() })
        .child(animated_layout)
        .into_any_element()
}

#[derive(IntoElement)]
struct SpringDemo {
    id: ElementId,
}

impl RenderOnce for SpringDemo {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = astryx_theme::current(window, cx);
        let t = theme.tokens();
        let card = use_animator(
            ElementId::Name(format!("{}-card", self.id).into()),
            window,
            cx,
        );
        let at_end = card.get(cx).x > 120.;
        let mover = card.clone();
        let shrink = card.clone();
        Stack::vertical()
            .gap(SpacingStep::S2)
            .child(
                div().w(px(420.)).h(px(64.)).child(
                    card.apply(
                        div()
                            .size(px(64.))
                            .rounded(t.radius.element)
                            .bg(t.color.accent),
                        window,
                        cx,
                    ),
                ),
            )
            .child(
                div()
                    .flex()
                    .gap(px(8.))
                    .child(
                        Button::new(ElementId::Name(format!("{}-move", self.id).into()), "Move")
                            .on_click(move |_, _, cx| {
                                let x = if mover.get(cx).x > 120. { 0. } else { 340. };
                                mover.animate(AnimatorValues::new().x(x), Default::default(), cx);
                            }),
                    )
                    .child(
                        Button::new(
                            ElementId::Name(format!("{}-pulse", self.id).into()),
                            "Shrink and fade",
                        )
                        .on_click(move |_, _, cx| {
                            let small = shrink.get(cx).scale < 0.9;
                            let (scale, opacity) = if small { (1., 1.) } else { (0.6, 0.4) };
                            shrink.animate(
                                AnimatorValues::new().scale(scale).opacity(opacity),
                                Default::default(),
                                cx,
                            );
                        }),
                    )
                    .child(
                        Text::new(if at_end { "At the end" } else { "At the start" })
                            .text_type(TextType::Supporting),
                    ),
            )
    }
}

#[derive(IntoElement)]
struct EntryDemo {
    id: ElementId,
}

impl RenderOnce for EntryDemo {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let count = window.use_keyed_state(
            ElementId::Name(format!("{}-count", self.id).into()),
            cx,
            |_, _| 2usize,
        );
        let n = *count.read(cx);
        let presets = [
            EntryAnimationPreset::SlideDown,
            EntryAnimationPreset::SlideUp,
            EntryAnimationPreset::FadeIn,
            EntryAnimationPreset::ScaleIn,
        ];
        let mut list = Stack::vertical().gap(SpacingStep::S1);
        for i in 0..n {
            let preset = presets[i % presets.len()];
            let entry = use_entry_animation(
                ElementId::Name(format!("{}-row-{i}", self.id).into()),
                preset,
                window,
                cx,
            );
            list = list.child(entry.apply(
                div().child(Text::new(format!("Row {} ({preset:?})", i + 1))),
                window,
                cx,
            ));
        }
        Stack::vertical()
            .gap(SpacingStep::S2)
            .child(
                div().flex().child(
                    Button::new(
                        ElementId::Name(format!("{}-add", self.id).into()),
                        "Add a row",
                    )
                    .on_click(move |_, window, cx| {
                        count.update(cx, |n, _| *n += 1);
                        window.refresh();
                    }),
                ),
            )
            .child(list)
    }
}
