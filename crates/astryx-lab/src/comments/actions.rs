//! A comment's actions (upstream lab `Comments/CommentActions.tsx`): Mark as resolved
//! or Reopen, Add reaction, Reply and Copy link as icon buttons in the header's top-end
//! corner, then the more menu; the reaction summary and the reaction picker.

use std::rc::Rc;
use std::time::Duration;

use astryx_components::button::{Button, ButtonVariant, IconButton, ToggleButton};
use astryx_components::icon::{Icon, IconColor};
use astryx_components::overflow::{CollapseFrom, OverflowList};
use astryx_components::overlay::{DropdownMenu, MenuEntry, MenuItem, Popover, Tooltip};
use astryx_components::text::{Text, TextColor, TextType, TextWeight};
use astryx_i18n::Arg;
use astryx_primitives::{ElementSize, LayerAlign, Placement, Politeness, announce};
use gpui::prelude::*;
use gpui::{AnyElement, App, ClipboardItem, ElementId, SharedString, Window, div};

use super::context::{Scope, scoped};
use super::i18n::reaction_description;
use super::icons::{CommentIcon, CommentIconName};
use super::types::{ActionFn, CommentData, CommentReaction, FlagFn};
use super::utils::comment_anchor_id;

/// How long a copied (or failed) link reads so (upstream `useClipboardCopy`'s reset).
const COPY_RESET: Duration = Duration::from_millis(2000);

/// Upstream `CommentActionId`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum CommentActionId {
    Resolve,
    Reopen,
    React,
    Reply,
    Link,
}

impl CommentActionId {
    fn as_str(self) -> &'static str {
        match self {
            Self::Resolve => "resolve",
            Self::Reopen => "reopen",
            Self::React => "react",
            Self::Reply => "reply",
            Self::Link => "link",
        }
    }

    fn of(key: &str) -> Option<Self> {
        Some(match key {
            "resolve" => Self::Resolve,
            "reopen" => Self::Reopen,
            "react" => Self::React,
            "reply" => Self::Reply,
            "link" => Self::Link,
            _ => return None,
        })
    }
}

/// Upstream `ResolveAction`: Mark as resolved on an open thread, Reopen on a resolved
/// one.
#[derive(Clone)]
pub(crate) struct ResolveAction {
    pub reopen: bool,
    pub on_click: ActionFn,
}

/// Who reacted, under the emoji and count (upstream `ReactionTooltipContent`).
fn reaction_tooltip(reaction: &CommentReaction, cx: &App) -> SharedString {
    let i18n = astryx_i18n::current(cx);
    let mut names: Vec<SharedString> = reaction
        .actors
        .iter()
        .map(|(_, name)| name.clone())
        .collect();
    let people = if names.is_empty() {
        reaction_description(&i18n, &reaction.option)
    } else {
        let others = reaction.count as i64 - names.len() as i64;
        if others > 0 {
            names.push(i18n.format(
                "@astryx.comments.otherReactors",
                &[("count", Arg::Num(others as f64))],
            ));
        }
        let locale = astryx_i18n::use_locale(cx);
        let parts: Vec<&str> = names.iter().map(|n| n.as_ref()).collect();
        crate::chat::long_conjunction(&locale, &parts).into()
    };
    format!("{} {}\n{}", reaction.option.label, reaction.count, people).into()
}

/// Upstream `ReactionSummary`: a comment's reactions, a toggle for each, pressed where
/// the viewer reacted.
pub(crate) fn reaction_summary(
    scope: &Scope,
    comment: &CommentData,
    can_react: bool,
    busy: bool,
    cx: &App,
) -> Option<AnyElement> {
    if comment.reactions.is_empty() {
        return None;
    }
    let i18n = astryx_i18n::current(cx);
    let reactions_selector = format!("comment-reactions-{}", comment.id);
    let mut row = div()
        .flex()
        .items_center()
        .gap(gpui::px(4.))
        .debug_selector(move || reactions_selector);
    for reaction in &comment.reactions {
        let label = i18n.format(
            "@astryx.comments.reactionCount",
            &[
                (
                    "reaction",
                    Arg::from(reaction_description(&i18n, &reaction.option).as_ref()),
                ),
                ("count", Arg::Num(reaction.count as f64)),
            ],
        );
        let (scope, comment, key) = (scope.clone(), comment.clone(), reaction.option.key.clone());
        let id = scoped(
            &scope.id,
            format!("reaction-{}-{}", comment.id, reaction.option.key),
        );
        let selector = format!("comment-reaction-{}-{}", comment.id, reaction.option.key);
        // A pending reaction stays enabled, so focus never drops mid-request; the
        // press is refused instead.
        let toggle = ToggleButton::new(id.clone(), label)
            .size(ElementSize::Sm)
            .pressed(reaction.reacted_by_viewer)
            .disabled(!can_react)
            .icon(
                div()
                    .flex()
                    .items_center()
                    .gap(gpui::px(4.))
                    .child(
                        Text::new(reaction.option.label.clone())
                            .text_type(TextType::Body)
                            .color(TextColor::Primary)
                            .weight(TextWeight::Normal),
                    )
                    .child(
                        Text::new(reaction.count.to_string())
                            .text_type(TextType::Body)
                            .color(TextColor::Primary)
                            .weight(TextWeight::Normal),
                    ),
            )
            .on_pressed_change(move |_, _, window, cx| {
                if !busy {
                    scope.react(&comment, key.clone(), None, window, cx);
                }
            });
        row = row.child(
            Tooltip::new(
                ElementId::Name(format!("{id}-tooltip").into()),
                reaction_tooltip(reaction, cx),
            )
            .child(div().debug_selector(move || selector).child(toggle)),
        );
    }
    Some(row.into_any_element())
}

/// What the actions keep across frames.
#[derive(Default)]
struct ActionsState {
    picker_open: bool,
    collapsed: Vec<CommentActionId>,
    /// The copied (`true`) or failed (`false`) state, and its reset task.
    copied: Option<bool>,
    reset: Option<gpui::Task<()>>,
}

/// What a comment's header actions show (upstream `CommentActions`'s props).
pub(crate) struct ActionsProps {
    pub comment: CommentData,
    pub can_copy_link: bool,
    pub can_react: bool,
    pub can_reply: bool,
    pub busy: bool,
    /// Edit and Delete, where the viewer may use them.
    pub menu_items: Vec<MenuEntry>,
    pub menu_label: SharedString,
    /// Pins the revealed actions while the picker or the menu is open.
    pub on_open_change: FlagFn,
    pub on_reply: ActionFn,
    pub resolve: Option<ResolveAction>,
    pub additional: Option<AnyElement>,
}

/// Upstream `CommentActions`: when the header runs out of room the buttons collapse
/// into the more menu, Copy link first, beside Edit and Delete.
pub(crate) fn comment_actions(
    scope: &Scope,
    props: ActionsProps,
    window: &mut Window,
    cx: &mut App,
) -> AnyElement {
    let i18n = astryx_i18n::current(cx);
    let comment_id = props.comment.id.clone();
    let state = window.use_keyed_state(
        scoped(&scope.id, format!("actions-{comment_id}")),
        cx,
        |_, _| ActionsState::default(),
    );
    let (picker_open, collapsed, copied) = {
        let s = state.read(cx);
        (s.picker_open, s.collapsed.clone(), s.copied)
    };
    let link_failed_label = i18n.t("@astryx.comments.linkCopyFailed");
    let link_label = match copied {
        Some(false) => link_failed_label.clone(),
        Some(true) => i18n.t("@astryx.comments.linkCopied"),
        None => i18n.t("@astryx.comments.copyLink"),
    };
    let copy: ActionFn = {
        let (state, href, failed) = (
            state.clone(),
            format!("#{}", comment_anchor_id(&comment_id)),
            link_failed_label.clone(),
        );
        Rc::new(move |window, cx| {
            cx.write_to_clipboard(ClipboardItem::new_string(href.clone()));
            let ok = cx
                .read_from_clipboard()
                .and_then(|c| c.text())
                .is_some_and(|t| t == href);
            if !ok {
                announce(failed.clone(), Politeness::Assertive, window, cx);
            }
            let timer = cx.background_executor().timer(COPY_RESET);
            let weak = state.downgrade();
            let reset = window.spawn(cx, async move |cx| {
                timer.await;
                let _ = cx.update(|_, cx| {
                    if let Some(state) = weak.upgrade() {
                        state.update(cx, |s, cx| {
                            s.copied = None;
                            cx.notify();
                        });
                    }
                });
            });
            state.update(cx, |s, cx| {
                s.copied = Some(ok);
                s.reset = Some(reset);
                cx.notify();
            });
        })
    };
    let change_picker: FlagFn = {
        let (state, report) = (state.clone(), props.on_open_change.clone());
        Rc::new(move |open, window, cx| {
            state.update(cx, |s, cx| {
                s.picker_open = open;
                cx.notify();
            });
            report(open, window, cx);
        })
    };
    let resolve_label = i18n.t("@astryx.comments.resolve");
    let reopen_label = i18n.t("@astryx.comments.reopen");
    let reply_label = i18n.t("@astryx.comments.reply");
    let add_reaction_label = i18n.t("@astryx.comments.addReaction");
    let busy = props.busy;
    let has = |a: CommentActionId| collapsed.contains(&a);

    // The more menu: the collapsed actions, then the comment's own.
    let mut collapsed_items: Vec<MenuEntry> = Vec::new();
    if let Some(resolve) = &props.resolve {
        let kind = if resolve.reopen {
            CommentActionId::Reopen
        } else {
            CommentActionId::Resolve
        };
        if has(kind) {
            let f = resolve.on_click.clone();
            let mut item = MenuItem::new(
                kind.as_str(),
                if resolve.reopen {
                    reopen_label.clone()
                } else {
                    resolve_label.clone()
                },
            )
            .disabled(busy)
            .on_click(move |window, cx| f(window, cx));
            if !resolve.reopen {
                item = item.icon("check");
            }
            collapsed_items.push(MenuEntry::Item(item));
        }
    }
    if props.can_reply && has(CommentActionId::Reply) {
        let f = props.on_reply.clone();
        collapsed_items.push(MenuEntry::Item(
            MenuItem::new("reply", reply_label.clone())
                .disabled(busy)
                .on_click(move |window, cx| f(window, cx)),
        ));
    }
    // A comment that already has a more menu keeps Copy link in it; elsewhere Copy
    // link is a header button until the header runs out of room.
    let link_in_menu = !props.menu_items.is_empty();
    let copy_item = || {
        let f = copy.clone();
        MenuEntry::Item(
            MenuItem::new("copy-link", link_label.clone())
                .on_click(move |window, cx| f(window, cx)),
        )
    };
    if props.can_copy_link && !link_in_menu && has(CommentActionId::Link) {
        collapsed_items.push(copy_item());
    }
    if props.can_react && has(CommentActionId::React) {
        let entries: Vec<MenuEntry> = scope
            .reaction_options
            .iter()
            .map(|option| {
                let (scope, comment, option) =
                    (scope.clone(), props.comment.clone(), option.clone());
                MenuEntry::Item(
                    MenuItem::new(
                        format!("react-{}", option.key),
                        reaction_description(&i18n, &option),
                    )
                    .description(option.label.clone())
                    .disabled(busy)
                    .on_click(move |window, cx| {
                        scope.react(
                            &comment,
                            option.key.clone(),
                            Some(option.clone()),
                            window,
                            cx,
                        )
                    }),
                )
            })
            .collect();
        collapsed_items.push(MenuEntry::section(add_reaction_label.clone(), entries));
    }
    let mut own: Vec<MenuEntry> = Vec::new();
    if props.can_copy_link && link_in_menu {
        own.push(copy_item());
    }
    own.extend(props.menu_items);
    let mut items = collapsed_items;
    if !items.is_empty() && !own.is_empty() {
        items.push(MenuEntry::Divider);
    }
    items.extend(own);

    // The buttons, Mark as resolved first, so it is the last to collapse.
    let mut list = OverflowList::new(scoped(&scope.id, format!("actions-list-{comment_id}")))
        .collapse_from(CollapseFrom::End)
        .on_overflow_change({
            let state = state.clone();
            move |items, _, cx| {
                let next: Vec<CommentActionId> = items
                    .iter()
                    .filter_map(|i| i.key.as_deref().and_then(CommentActionId::of))
                    .collect();
                state.update(cx, |s, cx| {
                    if s.collapsed != next {
                        s.collapsed = next;
                        cx.notify();
                    }
                });
            }
        });
    let sid = scope.id.clone();
    if let Some(resolve) = props.resolve.clone() {
        let (label, kind) = if resolve.reopen {
            (reopen_label.clone(), CommentActionId::Reopen)
        } else {
            (resolve_label.clone(), CommentActionId::Resolve)
        };
        let id = scoped(&sid, format!("{}-{comment_id}", kind.as_str()));
        let cid_resolve = comment_id.clone();
        list = list.keyed_item(kind.as_str(), move |_, _| {
            let f = resolve.on_click.clone();
            let selector = format!("comment-action-{}-{}", kind.as_str(), cid_resolve);
            let button: AnyElement = if resolve.reopen {
                Button::new(id.clone(), label.clone())
                    .variant(ButtonVariant::Ghost)
                    .size(ElementSize::Sm)
                    .disabled(busy)
                    .on_click(move |_, window, cx| f(window, cx))
                    .into_any_element()
            } else {
                IconButton::new(
                    id.clone(),
                    label.clone(),
                    CommentIcon::new(CommentIconName::Resolve),
                )
                .tooltip(label.clone())
                .variant(ButtonVariant::Ghost)
                .size(ElementSize::Sm)
                .disabled(busy)
                .on_click(move |_, window, cx| f(window, cx))
                .into_any_element()
            };
            div()
                .flex()
                .items_center()
                .debug_selector(move || selector.clone())
                .child(button)
                .into_any_element()
        });
    }
    if props.can_react {
        let (label, id, change, open) = (
            add_reaction_label.clone(),
            scoped(&sid, format!("react-{comment_id}")),
            change_picker.clone(),
            picker_open,
        );
        let sel = format!("comment-action-react-{comment_id}");
        list = list.keyed_item("react", move |_, _| {
            let change = change.clone();
            let mut button = IconButton::new(
                id.clone(),
                label.clone(),
                CommentIcon::new(CommentIconName::AddReaction),
            )
            .variant(ButtonVariant::Ghost)
            .size(ElementSize::Sm)
            .disabled(busy)
            .expanded(open)
            .on_click(move |_, window, cx| change(!open, window, cx));
            // The tooltip rests while the picker is open.
            if !open {
                button = button.tooltip(label.clone());
            }
            div()
                .flex()
                .items_center()
                .debug_selector({
                    let s = sel.clone();
                    move || s
                })
                .child(button)
                .into_any_element()
        });
    }
    if props.can_reply {
        let (label, id, f) = (
            reply_label.clone(),
            scoped(&sid, format!("reply-{comment_id}")),
            props.on_reply.clone(),
        );
        let sel = format!("comment-action-reply-{comment_id}");
        list = list.keyed_item("reply", move |_, _| {
            let f = f.clone();
            div()
                .flex()
                .items_center()
                .debug_selector({
                    let s = sel.clone();
                    move || s
                })
                .child(
                    IconButton::new(
                        id.clone(),
                        label.clone(),
                        CommentIcon::new(CommentIconName::Reply),
                    )
                    .tooltip(label.clone())
                    .variant(ButtonVariant::Ghost)
                    .size(ElementSize::Sm)
                    .disabled(busy)
                    .on_click(move |_, window, cx| f(window, cx)),
                )
                .into_any_element()
        });
    }
    if props.can_copy_link && !link_in_menu {
        let (label, id, f) = (
            link_label.clone(),
            scoped(&sid, format!("link-{comment_id}")),
            copy.clone(),
        );
        let icon_kind = copied;
        let sel = format!("comment-action-link-{comment_id}");
        list = list.keyed_item("link", move |_, _| {
            let f = f.clone();
            let icon: AnyElement = match icon_kind {
                Some(false) => Icon::new("close")
                    .color(IconColor::Secondary)
                    .into_any_element(),
                Some(true) => Icon::new("check")
                    .color(IconColor::Secondary)
                    .into_any_element(),
                None => CommentIcon::new(CommentIconName::CopyLink).into_any_element(),
            };
            div()
                .flex()
                .items_center()
                .debug_selector({
                    let s = sel.clone();
                    move || s
                })
                .child(
                    IconButton::new(id.clone(), label.clone(), icon)
                        .tooltip(label.clone())
                        .variant(ButtonVariant::Ghost)
                        .size(ElementSize::Sm)
                        .on_click(move |_, window, cx| f(window, cx)),
                )
                .into_any_element()
        });
    }
    let more = (!items.is_empty()).then(|| {
        let report = props.on_open_change.clone();
        let selector = format!("comment-more-{comment_id}");
        div().debug_selector(move || selector).child(
            DropdownMenu::new(scoped(&sid, format!("more-{comment_id}")))
                .items(items)
                .icon_only(true)
                .button_label(props.menu_label.clone())
                .button_variant(ButtonVariant::Ghost)
                .button_size(ElementSize::Sm)
                .button_icon(Icon::new("moreHorizontal").color(IconColor::Secondary))
                .chevron(false)
                .alignment(LayerAlign::End)
                .menu_width(gpui::px(160.))
                .on_open_change(move |open, window, cx| report(open, window, cx)),
        )
    });
    let cluster = div()
        .flex()
        .items_center()
        .gap(gpui::px(2.))
        .w_full()
        .debug_selector({
            let s = format!("comment-actions-{comment_id}");
            move || s
        })
        .child(div().flex().flex_1().min_w_0().justify_end().child(list))
        .children(props.additional)
        .children(more);
    if !props.can_react {
        return cluster.into_any_element();
    }
    // Only while open: a closed picker leaves no buttons in the page.
    let picker = picker_open.then(|| {
        let mut row = div()
            .flex()
            .items_center()
            .gap(gpui::px(2.))
            .debug_selector(|| "reaction-picker".into());
        for option in scope.reaction_options.iter() {
            let label = reaction_description(&i18n, option);
            let (scope, comment, option, change, state) = (
                scope.clone(),
                props.comment.clone(),
                option.clone(),
                change_picker.clone(),
                state.clone(),
            );
            let id = scoped(&sid, format!("pick-{}-{}", comment_id, option.key));
            let selector = format!("reaction-option-{}", option.key);
            row = row.child(
                div().debug_selector(move || selector).child(
                    IconButton::new(
                        id,
                        label.clone(),
                        Text::new(option.label.clone()).text_type(TextType::Large),
                    )
                    .tooltip(label)
                    .variant(ButtonVariant::Ghost)
                    .disabled(busy)
                    .on_click(move |_, window, cx| {
                        scope.react(
                            &comment,
                            option.key.clone(),
                            Some(option.clone()),
                            window,
                            cx,
                        );
                        change(false, window, cx);
                        let _ = &state;
                        // Focus returns to Add reaction, or the more menu once it
                        // has collapsed there.
                        scope.focus_target(
                            Some(&comment.id),
                            super::context::FocusTarget::Reply,
                            window,
                            cx,
                        );
                        astryx_primitives::request_focus(
                            &scoped(&scope.id, format!("react-{}", comment.id)),
                            cx,
                        );
                    }),
                ),
            );
        }
        row
    });
    let open_change = change_picker.clone();
    let mut popover = Popover::new(
        scoped(&sid, format!("picker-{comment_id}")),
        i18n.t("@astryx.comments.chooseReaction"),
    )
    .open(picker_open)
    .placement(Placement::Above)
    .alignment(LayerAlign::End)
    .on_open_change(move |open, window, cx| open_change(open, window, cx))
    .child(cluster);
    if let Some(picker) = picker {
        popover = popover.content(picker);
    }
    popover.into_any_element()
}
