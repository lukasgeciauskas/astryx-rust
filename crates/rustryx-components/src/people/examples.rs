//! People examples: the gallery's People page and the doc example of
//! Avatar and AvatarGroup.

use gpui::{AnyElement, App, IntoElement, ParentElement, Window};

use crate::layout::{SpacingStep, Stack, StackAlign};
use crate::people::{
    Avatar, AvatarGroup, AvatarGroupOverflow, AvatarShape, AvatarSize, AvatarStatusDot,
    AvatarStatusVariant,
};
use crate::text::{Text, TextType};

pub fn examples(_window: &mut Window, _cx: &mut App) -> AnyElement {
    let label = |text: &'static str| Text::new(text).text_type(TextType::Supporting);
    let row = || {
        Stack::horizontal()
            .gap(SpacingStep::S4)
            .v_align(StackAlign::Center)
    };
    let sizes = [
        AvatarSize::Xsm,
        AvatarSize::Sm,
        AvatarSize::Md,
        AvatarSize::Lg,
        AvatarSize::Xl,
    ];
    Stack::vertical()
        .gap(SpacingStep::S4)
        .child(label("Initials at every size, and the icon fallback"))
        .child(
            row()
                .children(
                    sizes
                        .into_iter()
                        .enumerate()
                        .map(|(i, size)| Avatar::new(("size", i)).name("Ada Lovelace").size(size)),
                )
                .child(Avatar::new("anonymous").size(AvatarSize::Lg)),
        )
        .child(label("Shapes"))
        .child(
            row()
                .child(
                    Avatar::new("circle")
                        .name("Grace Hopper")
                        .size(AvatarSize::Lg),
                )
                .child(
                    Avatar::new("rounded")
                        .name("Grace Hopper")
                        .size(AvatarSize::Lg)
                        .shape(AvatarShape::Rounded),
                )
                .child(
                    Avatar::new("square")
                        .name("Grace Hopper")
                        .size(AvatarSize::Lg)
                        .shape(AvatarShape::Square),
                ),
        )
        .child(label("Status dots: online, away (ring), offline (minus)"))
        .child(
            row()
                .child(
                    Avatar::new("online").name("Alan Turing").status(
                        AvatarStatusDot::new("online-dot", AvatarStatusVariant::Success)
                            .label("Online"),
                    ),
                )
                .child(
                    Avatar::new("away")
                        .name("Alan Turing")
                        .size(AvatarSize::Lg)
                        .status(
                            AvatarStatusDot::new("away-dot", AvatarStatusVariant::Neutral)
                                .label("Away"),
                        ),
                )
                .child(
                    Avatar::new("offline")
                        .name("Alan Turing")
                        .size(AvatarSize::Xl)
                        .status(
                            AvatarStatusDot::new("offline-dot", AvatarStatusVariant::Error)
                                .label("Offline"),
                        ),
                ),
        )
        .child(label("AvatarGroup with an overflow chip"))
        .child(
            AvatarGroup::new("team")
                .child(Avatar::new("a").name("Ada Lovelace"))
                .child(Avatar::new("b").name("Grace Hopper"))
                .child(Avatar::new("c").name("Alan Turing"))
                .child(Avatar::new("d").name("Katherine Johnson"))
                .child(AvatarGroupOverflow::new(5)),
        )
        .into_any_element()
}
