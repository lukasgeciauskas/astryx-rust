//! Swipe examples: mail rows whose verbs a sideways swipe uncovers, each also reachable
//! from the row's own menu. A finger, a pen or a trackpad's two-finger swipe drags a
//! row; a mouse never does.

use std::rc::Rc;

use gpui::{AnyElement, App, IntoElement, ParentElement, SharedString, Styled, Window, px};

use crate::icon::{Icon, IconSize};
use crate::layout::{SpacingStep, Stack};
use crate::list::{List, ListItem};
use crate::overlay::MenuItem;
use crate::swipe::{
    CreateSwipeOptions, ItemSwipeActionVariant, ItemSwipeBehavior, SwipeAction, create_swipe,
};
use crate::text::{Text, TextType};

/// The verbs of a mail row.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Mail {
    Unread,
    Flag,
    Archive,
    Delete,
}

pub fn examples(window: &mut Window, cx: &mut App) -> AnyElement {
    let inbox = window.use_keyed_state("swipe-example-inbox", cx, |_, _| {
        vec![
            ("Ada", "Lunch on Thursday?"),
            ("Grace", "The compiler is ready"),
            ("Linus", "Patch review"),
            ("Margaret", "Launch checklist"),
        ]
    });
    let last = window.use_keyed_state("swipe-example-last", cx, |_, _| SharedString::from("—"));
    let report = {
        let last = last.clone();
        Rc::new(move |text: String, cx: &mut App| last.update(cx, |l, _| *l = text.into()))
    };
    // One definition serves every row: each handler reads the row from its key.
    let mail = {
        let (inbox, report) = (inbox.clone(), report.clone());
        let r = report.clone();
        create_swipe(
            CreateSwipeOptions::new()
                .leading([SwipeAction::new(Mail::Unread, "Unread", move |a, _, cx| {
                    r(format!("{:?} on {:?}", a.id, a.item_key), cx)
                })
                .icon(|_, _| Icon::new("info").size(IconSize::Sm).into_any_element())])
                .trailing([
                    SwipeAction::new(Mail::Flag, "Flag", {
                        let r = report.clone();
                        move |a, _, cx| r(format!("{:?} on {:?}", a.id, a.item_key), cx)
                    })
                    .variant(ItemSwipeActionVariant::Neutral)
                    .icon(|_, _| Icon::new("warning").size(IconSize::Sm).into_any_element()),
                    SwipeAction::new(Mail::Delete, "Delete", move |a, window, cx| {
                        report(format!("{:?} on {:?}", a.id, a.item_key), cx);
                        if let Some(crate::reorderable::ItemKey::Number(n)) = a.item_key {
                            inbox.update(cx, |rows, _| {
                                if (n as usize) < rows.len() {
                                    rows.remove(n as usize);
                                }
                            });
                            window.refresh();
                        }
                    })
                    .variant(ItemSwipeActionVariant::Destructive)
                    .has_removal(true)
                    .icon(|_, _| Icon::new("close").size(IconSize::Sm).into_any_element()),
                ]),
        )
    };
    let commit = create_swipe(
        CreateSwipeOptions::new()
            .trailing([SwipeAction::new(Mail::Archive, "Archive", {
                let report = report.clone();
                move |a, _, cx| report(format!("{:?} (commit)", a.id), cx)
            })])
            .swipe_behavior(ItemSwipeBehavior::Commit),
    );
    let rows: Vec<(&str, &str)> = inbox.read(cx).clone();
    let mut list = List::new("swipe-inbox").dividers(true).w(px(340.));
    for (i, (from, subject)) in rows.into_iter().enumerate() {
        list = list.child(
            ListItem::new(SharedString::from(format!("mail-{from}")), from)
                .description(subject)
                .item_key(i)
                .swipe(mail.clone())
                // Every verb has a second home in the row: its menu.
                .context_menu([
                    MenuItem::new("unread", "Mark unread").into(),
                    MenuItem::new("flag", "Flag").into(),
                    MenuItem::new("delete", "Delete").into(),
                ]),
        );
    }
    Stack::vertical()
        .gap(SpacingStep::S3)
        .child(Text::new("Swipe actions (createSwipe)").text_type(TextType::Supporting))
        .child(list)
        .child(
            List::new("swipe-commit").w(px(340.)).child(
                ListItem::new("commit-row", "Swipe to archive")
                    .description("commit: a full swipe fires")
                    .swipe(commit),
            ),
        )
        .child(Text::new(format!("Last action: {}", last.read(cx))).text_type(TextType::Supporting))
        .into_any_element()
}
