//! The CommentPin gallery (upstream's `Lab/CommentPin` stories). The participants show
//! their initials: the gallery loads no remote photos.

use gpui::{
    AnyElement, App, ElementId, IntoElement, ParentElement, Styled, Window, div, px, relative,
};
use rustryx_components::layout::{SpacingStep, Stack, StackAlign};
use rustryx_components::text::{Text, TextType};
use rustryx_i18n::{LogicalStyled, use_direction};

use super::{CommentPin, CommentPinParticipant, CommentPinSize};

fn example(title: &'static str, body: impl IntoElement) -> AnyElement {
    Stack::vertical()
        .gap(SpacingStep::S2)
        .child(Text::new(title).text_type(TextType::Large))
        .child(body)
        .into_any_element()
}

fn ada() -> CommentPinParticipant {
    CommentPinParticipant::new("ada", "Ada Lovelace")
}
fn lin() -> CommentPinParticipant {
    CommentPinParticipant::new("lin", "Lin Chen")
}
fn mira() -> CommentPinParticipant {
    CommentPinParticipant::new("mira", "Mira Patel")
}
fn everyone() -> Vec<CommentPinParticipant> {
    vec![
        ada(),
        lin(),
        mira(),
        CommentPinParticipant::new("noah", "Noah Williams"),
        CommentPinParticipant::new("sam", "Sam Rivera"),
    ]
}

fn row(label: impl Into<gpui::SharedString>, content: impl IntoElement) -> impl IntoElement {
    Stack::horizontal()
        .gap(SpacingStep::S4)
        .v_align(StackAlign::Center)
        .min_h(px(44.))
        .child(
            div()
                .min_w(px(140.))
                .child(Text::new(label).text_type(TextType::Supporting)),
        )
        .child(content)
}

/// Upstream's `SIZE_ROWS`: each state at every size.
fn sizes() -> impl IntoElement {
    type Build = fn(CommentPinSize, &str) -> CommentPin;
    let rows: [(&str, Build); 6] = [
        ("Single", |s, id| {
            CommentPin::new(id.to_string(), "Open annotation from Ada", [ada()]).size(s)
        }),
        ("Grouped", |s, id| {
            CommentPin::new(
                id.to_string(),
                "Open 4 grouped annotations from Ada and Lin",
                [ada(), lin()],
            )
            .thread_count(4)
            .size(s)
        }),
        ("Group overflow", |s, id| {
            CommentPin::new(
                id.to_string(),
                "Open 8 grouped annotations from Ada, Lin, and 3 others",
                everyone(),
            )
            .thread_count(8)
            .size(s)
        }),
        ("Unread single", |s, id| {
            CommentPin::new(id.to_string(), "Open unread annotation from Mira", [mira()])
                .unread_count(1)
                .size(s)
        }),
        ("Unread group", |s, id| {
            CommentPin::new(
                id.to_string(),
                "Open 8 grouped annotations from Ada, Lin, and 3 others, 3 unread",
                everyone(),
            )
            .thread_count(8)
            .unread_count(3)
            .size(s)
        }),
        ("Selected group", |s, id| {
            CommentPin::new(
                id.to_string(),
                "Current group of 12 annotations from Ada, Lin, and 3 others",
                everyone(),
            )
            .thread_count(12)
            .selected(true)
            .size(s)
        }),
    ];
    let sizes = [CommentPinSize::Sm, CommentPinSize::Md, CommentPinSize::Lg];
    let header = Stack::horizontal()
        .gap(SpacingStep::S4)
        .v_align(StackAlign::Center)
        .min_h(px(44.))
        .child(
            div()
                .min_w(px(140.))
                .child(Text::new("Size").text_type(TextType::Supporting)),
        )
        .children(sizes.map(|s| {
            div()
                .min_w(px(120.))
                .child(Text::new(s.as_str()).text_type(TextType::Supporting))
        }));
    Stack::vertical()
        .gap(SpacingStep::S3)
        .p(px(32.))
        .child(header)
        .children(rows.into_iter().map(|(name, build)| {
            Stack::horizontal()
                .gap(SpacingStep::S4)
                .v_align(StackAlign::Center)
                .min_h(px(44.))
                .child(
                    div()
                        .min_w(px(140.))
                        .child(Text::new(name).text_type(TextType::Supporting)),
                )
                .children(sizes.map(|s| {
                    let id = format!("pin-size-{name}-{}", s.as_str());
                    div()
                        .min_w(px(120.))
                        .child(build(s, &id).on_click(|_, _, _| {}))
                }))
        }))
}

/// Upstream's `StateMatrix`.
fn states() -> impl IntoElement {
    Stack::vertical()
        .gap(SpacingStep::S3)
        .p(px(32.))
        .child(row(
            "Single",
            CommentPin::new("pin-single", "Open annotation from Ada", [ada()]),
        ))
        .child(row(
            "Selected",
            CommentPin::new("pin-selected", "Current annotation from Ada", [ada()]).selected(true),
        ))
        .child(row(
            "Other pin dimmed",
            CommentPin::new("pin-dimmed", "Open another annotation from Mira", [mira()])
                .dimmed(true),
        ))
        .child(row(
            "Grouped",
            CommentPin::new(
                "pin-grouped",
                "Open 4 grouped annotations from Ada and Lin",
                [ada(), lin()],
            )
            .thread_count(4),
        ))
        .child(row(
            "Selected group",
            CommentPin::new(
                "pin-selected-group",
                "Current group of 12 annotations from Ada, Lin, and Mira",
                [ada(), lin(), mira()],
            )
            .thread_count(12)
            .selected(true),
        ))
        .child(row(
            "Group overflow",
            CommentPin::new(
                "pin-overflow",
                "Open 8 grouped annotations from Ada, Lin, and 3 others",
                everyone(),
            )
            .thread_count(8),
        ))
        .child(row(
            "Unread single",
            CommentPin::new("pin-unread", "Open unread annotation from Mira", [mira()])
                .unread_count(1),
        ))
        .child(row(
            "Unread group",
            CommentPin::new(
                "pin-unread-group",
                "Open 8 grouped annotations from Ada, Lin, and 3 others, 3 unread",
                everyone(),
            )
            .thread_count(8)
            .unread_count(3),
        ))
        // An unselected resolved pin renders nothing.
        .child(row(
            "Resolved, selected",
            CommentPin::new(
                "pin-resolved",
                "Current resolved annotation from Mira",
                [mira()],
            )
            .resolved(true)
            .selected(true),
        ))
        .child(row(
            "Disabled / pending",
            CommentPin::new("pin-pending", "Pending annotation from Ada", [ada()])
                .selected(true)
                .disabled(true),
        ))
}

/// Which placement-story pin is selected.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Placed {
    Content,
    Margin,
}

/// Upstream's `OverContentAndInMargin`: a pin anchored over content and one in the
/// margin. Each maps its anchor point to the pin's bottom-start corner by moving up its
/// own height; pressing the selected one clears the selection (`aria-pressed`).
fn placement(window: &mut Window, cx: &mut App) -> impl IntoElement {
    let theme = rustryx_theme::current(window, cx);
    let t = theme.tokens();
    let dir = use_direction(cx);
    let selected = window.use_keyed_state(
        ElementId::Name("comment-pin-placement".into()),
        cx,
        |_, _| Some(Placed::Content),
    );
    let current = *selected.read(cx);
    let toggle = |which: Placed| {
        let selected = selected.clone();
        move |_: &gpui::ClickEvent, window: &mut Window, cx: &mut App| {
            selected.update(cx, |s, _| {
                *s = if *s == Some(which) { None } else { Some(which) }
            });
            window.refresh();
        }
    };
    let pin_height = t.size.element_sm;
    div()
        .relative()
        .w(px(520.))
        .h(px(300.))
        .overflow_hidden()
        .rounded(t.radius.container)
        .bg(t.color.background_surface)
        .child(
            // The content the pin annotates: a muted stand-in for upstream's photo.
            div()
                .absolute()
                .top_0()
                .left_0()
                .size_full()
                .bg(t.color.background_muted),
        )
        .child(
            div()
                .absolute()
                .inset_start(dir, relative(0.38))
                .top(relative(0.34))
                .mt(-pin_height.to_pixels(window.rem_size()))
                .child(
                    CommentPin::new(
                        "pin-content",
                        "Open 4 grouped annotations over the image",
                        [ada(), lin()],
                    )
                    .thread_count(4)
                    .selected(current == Some(Placed::Content))
                    .dimmed(current.is_some() && current != Some(Placed::Content))
                    .on_click(toggle(Placed::Content)),
                ),
        )
        .child(
            div()
                .absolute()
                .top_0()
                .bottom_0()
                .inset_end(dir, px(0.))
                .w(px(72.))
                .bg(t.color.background_muted)
                .border_color(t.color.border)
                .border_l_1()
                .child(
                    div()
                        .absolute()
                        .inset_start(dir, relative(0.5))
                        .top(relative(0.66))
                        .mt(-pin_height.to_pixels(window.rem_size()))
                        .child(
                            CommentPin::new(
                                "pin-margin",
                                "Open margin annotation from Mira",
                                [mira()],
                            )
                            .selected(current == Some(Placed::Margin))
                            .dimmed(current.is_some() && current != Some(Placed::Margin))
                            .on_click(toggle(Placed::Margin)),
                        ),
                ),
        )
}

/// The CommentPin examples: default, grouped, the size matrix, the state matrix, and
/// placement over content and in a margin.
pub fn examples(window: &mut Window, cx: &mut App) -> AnyElement {
    Stack::vertical()
        .gap(SpacingStep::S8)
        .child(example(
            "Default",
            div().child(
                CommentPin::new("pin-default", "Open annotation from Ada", [ada()])
                    .on_click(|_, _, _| {}),
            ),
        ))
        .child(example(
            "Grouped",
            div().child(
                CommentPin::new(
                    "pin-default-group",
                    "Open 4 grouped annotations from Ada and Lin",
                    [ada(), lin()],
                )
                .thread_count(4)
                .on_click(|_, _, _| {}),
            ),
        ))
        .child(example("Sizes", sizes()))
        .child(example("State matrix", states()))
        .child(example(
            "Over content and in a margin",
            placement(window, cx),
        ))
        .into_any_element()
}
