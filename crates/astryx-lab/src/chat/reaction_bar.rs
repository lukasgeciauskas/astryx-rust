//! `ChatReactionBar`: a row of emoji reaction pills under a chat message (upstream lab
//! `ChatReactionBar`): each pill an emoji and its count, pressed for the current user's own
//! reactions, with a tooltip naming who reacted when given; and, with `on_add`, a trailing
//! add-reaction button that opens a ChatEmojiPicker popover. Data flows in through the
//! reactions; toggles flow out through `on_toggle` and new reactions through `on_add`.

use std::rc::Rc;

use astryx_components::button::{Button, ButtonVariant};
use astryx_components::overlay::Tooltip;
use astryx_primitives::{control_focus, is_focus_visible};
use astryx_theme::{ThemeStyled, current};
use gpui::prelude::*;
use gpui::{
    AnyElement, App, ElementId, FontFeatures, IntoElement, Role, SharedString, StyleRefinement,
    Toggled, Window, div, px,
};

use super::emoji_picker::{ChatEmojiOption, ChatEmojiPicker};

/// One reaction (upstream `ChatReaction`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChatReaction {
    /// The emoji character, e.g. "🎉".
    pub emoji: SharedString,
    /// How many people reacted with it.
    pub count: usize,
    /// Upstream `isSelected`: the current user reacted (accent border and tint, pressed).
    pub selected: bool,
    /// Upstream `label`: the pill's tooltip and accessible name, e.g. "Ana and Dana
    /// reacted with 🎉"; a count-based name without it.
    pub label: Option<SharedString>,
}

impl ChatReaction {
    /// A reaction with its count, not the user's, unlabelled.
    pub fn new(emoji: impl Into<SharedString>, count: usize) -> Self {
        ChatReaction {
            emoji: emoji.into(),
            count,
            selected: false,
            label: None,
        }
    }

    /// Upstream `isSelected`.
    pub fn selected(mut self, selected: bool) -> Self {
        self.selected = selected;
        self
    }

    /// Upstream `label`.
    pub fn label(mut self, label: impl Into<SharedString>) -> Self {
        self.label = Some(label.into());
        self
    }
}

/// A pill's accessible name (upstream `pillLabel`): its label, else "N reaction(s) with
/// emoji".
pub fn pill_label(reaction: &ChatReaction) -> SharedString {
    if let Some(label) = &reaction.label {
        return label.clone();
    }
    let noun = if reaction.count == 1 {
        "reaction"
    } else {
        "reactions"
    };
    format!("{} {noun} with {}", reaction.count, reaction.emoji).into()
}

type EmojiFn = Rc<dyn Fn(SharedString, &mut Window, &mut App)>;

/// The reaction pills (upstream lab `ChatReactionBar`).
///
/// ```ignore
/// ChatReactionBar::new("reactions", [ChatReaction::new("🎉", 4).selected(true)])
///     .on_toggle(|emoji, _, _| { /* toggle it */ })
///     .on_add(|emoji, _, _| { /* add it */ })
/// ```
#[derive(IntoElement)]
pub struct ChatReactionBar {
    id: ElementId,
    reactions: Vec<ChatReaction>,
    on_toggle: Option<EmojiFn>,
    on_add: Option<EmojiFn>,
    emojis: Option<Vec<ChatEmojiOption>>,
    add_label: SharedString,
    label: SharedString,
    style: StyleRefinement,
}

impl ChatReactionBar {
    /// A bar of `reactions`, in display order (upstream `reactions`).
    pub fn new(
        id: impl Into<ElementId>,
        reactions: impl IntoIterator<Item = ChatReaction>,
    ) -> Self {
        ChatReactionBar {
            id: id.into(),
            reactions: reactions.into_iter().collect(),
            on_toggle: None,
            on_add: None,
            emojis: None,
            add_label: "Add reaction".into(),
            label: "Reactions".into(),
            style: StyleRefinement::default(),
        }
    }

    /// Upstream `onToggle`: the pill's emoji when the user toggles a reaction.
    pub fn on_toggle(mut self, f: impl Fn(SharedString, &mut Window, &mut App) + 'static) -> Self {
        self.on_toggle = Some(Rc::new(f));
        self
    }

    /// Upstream `onAdd`: the emoji picked from the picker; the add-reaction button renders
    /// only with it.
    pub fn on_add(mut self, f: impl Fn(SharedString, &mut Window, &mut App) + 'static) -> Self {
        self.on_add = Some(Rc::new(f));
        self
    }

    /// Upstream `emojis`: the picker's options (the default chat set by default).
    pub fn emojis(mut self, emojis: impl IntoIterator<Item = ChatEmojiOption>) -> Self {
        self.emojis = Some(emojis.into_iter().collect());
        self
    }

    /// Upstream `addLabel`: the add-reaction button's name ("Add reaction").
    pub fn add_label(mut self, label: impl Into<SharedString>) -> Self {
        self.add_label = label.into();
        self
    }

    /// Upstream `label`: the group's name ("Reactions").
    pub fn label(mut self, label: impl Into<SharedString>) -> Self {
        self.label = label.into();
        self
    }
}

impl Styled for ChatReactionBar {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

/// What a bar drew, for tests: its name, each pill's name, pressed state and count, and
/// the add button's name.
#[cfg(test)]
#[derive(Clone, Debug, Default)]
pub(crate) struct BarDrawn {
    pub label: SharedString,
    pub pills: Vec<(SharedString, bool, usize, bool)>,
    pub add: Option<SharedString>,
}

#[cfg(test)]
thread_local! {
    pub(crate) static BARS: std::cell::RefCell<std::collections::HashMap<String, BarDrawn>> = Default::default();
}

impl RenderOnce for ChatReactionBar {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = current(window, cx);
        let t = theme.tokens().clone();
        let rem = window.rem_size();
        let leading = |size: gpui::AbsoluteLength, lead: f32| size.to_pixels(rem) * lead;
        let tabular = FontFeatures(std::sync::Arc::new(vec![("tnum".into(), 1)]));
        #[cfg(test)]
        let mut drawn = BarDrawn {
            label: self.label.clone(),
            ..Default::default()
        };
        let mut children: Vec<AnyElement> = Vec::new();
        for reaction in &self.reactions {
            let selected = reaction.selected;
            let (toggle, emoji) = (self.on_toggle.clone(), reaction.emoji.clone());
            let pill_id = ElementId::Name(format!("{}-pill-{}", self.id, reaction.emoji).into());
            let focus = control_focus(&pill_id, window, cx);
            let selector = format!("{}-pill-{}", self.id, reaction.emoji);
            let emphasized = t.color.border_emphasized;
            let mut pill = div()
                .id(pill_id)
                .debug_selector(move || selector)
                .track_focus(&focus)
                .relative()
                .flex()
                .items_center()
                .gap(t.spacing.s1)
                .h(px(24.))
                .px(t.spacing.s2)
                .rounded_full()
                .border_1()
                .border_color(gpui::transparent_black())
                .bg(t.color.background_muted)
                .cursor_pointer()
                .font_stack(&t.font.family.body)
                .role(Role::Button)
                .aria_label(pill_label(reaction))
                .aria_toggled(if selected {
                    Toggled::True
                } else {
                    Toggled::False
                })
                .on_click(move |_, window, cx| {
                    if let Some(f) = &toggle {
                        f(emoji.clone(), window, cx);
                    }
                });
            pill = if selected {
                pill.bg(t.color.accent_muted).border_color(t.color.accent)
            } else {
                pill.hover(move |s| s.border_color(emphasized))
            };
            pill = pill
                .child(
                    div()
                        .inert(true)
                        .text_size(t.text.body.size)
                        .line_height(leading(t.text.body.size, t.text.body.leading))
                        .child(reaction.emoji.clone()),
                )
                .child(
                    div()
                        .text_size(t.text.supporting.size)
                        .line_height(leading(t.text.supporting.size, t.text.supporting.leading))
                        .font_weight(if selected {
                            t.font.weight.semibold
                        } else {
                            t.font.weight.medium
                        })
                        .text_color(if selected {
                            t.color.text_accent
                        } else {
                            t.color.text_secondary
                        })
                        .font_features(tabular.clone())
                        .child(reaction.count.to_string()),
                );
            if is_focus_visible(&focus, window) {
                pill = pill.child(astryx_components::focus::focus_ring(&t, px(9999.), rem));
            }
            #[cfg(test)]
            drawn.pills.push((
                pill_label(reaction),
                selected,
                reaction.count,
                reaction.label.is_some(),
            ));
            children.push(match &reaction.label {
                Some(label) => Tooltip::new(
                    ElementId::Name(format!("{}-tip-{}", self.id, reaction.emoji).into()),
                    label.clone(),
                )
                .child(pill)
                .into_any_element(),
                None => pill.into_any_element(),
            });
        }
        if let Some(on_add) = self.on_add.clone() {
            #[cfg(test)]
            {
                drawn.add = Some(self.add_label.clone());
            }
            // The trigger is a Button, which the picker's popover wires up; its tooltip
            // names it as upstream's Tooltip does.
            let selector = format!("{}-add", self.id);
            let mut button = Button::new(
                ElementId::Name(format!("{}-add", self.id).into()),
                self.add_label.clone(),
            )
            .variant(ButtonVariant::Ghost)
            .icon_only(true)
            .tooltip(self.add_label.clone())
            .icon(
                div()
                    .debug_selector(move || selector)
                    .text_size(px(15.))
                    .line_height(leading(t.text.supporting.size, t.text.supporting.leading))
                    .child("+"),
            );
            button = button
                .w(px(32.))
                .h(px(24.))
                .p_0()
                .rounded_full()
                .border_1()
                .border_color(gpui::transparent_black())
                .bg(t.color.background_muted)
                .text_color(t.color.icon_secondary);
            let mut picker =
                ChatEmojiPicker::new(ElementId::Name(format!("{}-picker", self.id).into()))
                    .label(self.add_label.clone())
                    .on_select(move |emoji, window, cx| on_add(emoji, window, cx))
                    .child(button);
            if let Some(emojis) = &self.emojis {
                picker = picker.emojis(emojis.clone());
            }
            children.push(picker.into_any_element());
        }
        #[cfg(test)]
        BARS.with(|b| b.borrow_mut().insert(self.id.to_string(), drawn));
        let mut root = div().flex().flex_wrap().items_center().gap(t.spacing.s1);
        theme.refine(root.style(), "chat-reaction-bar", &[], None);
        root.style().refine(&self.style);
        root.id(self.id.clone())
            .role(Role::Group)
            .aria_label(self.label.clone())
            .children(children)
    }
}
