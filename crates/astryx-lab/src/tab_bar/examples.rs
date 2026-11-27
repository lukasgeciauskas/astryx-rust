//! The TabBar gallery (upstream's `TabBar` stories): four tabs with a badge and a
//! reselect counter, badges that appear and change at once, a disabled tab, and six tabs
//! whose last two move into More. Each bar floats over the bottom of a phone-sized
//! screen.

use astryx_components::button::Button;
use astryx_components::layout::{SpacingStep, Stack};
use astryx_components::text::{Text, TextType};
use astryx_theme::current;
use gpui::{
    AnyElement, App, Entity, IntoElement, ParentElement, SharedString, Styled, Window, div, px,
};

use super::{TabBar, TabBarItem};
use crate::tab_bar_more_list::TabBarMoreList;

/// The sections the examples use: value, label and icon.
const SECTIONS: [(&str, &str, &str); 6] = [
    ("today", "Today", "calendar"),
    ("recent", "Recent", "clock"),
    ("search", "Search", "search"),
    ("tools", "Tools", "wrench"),
    ("info", "Info", "info"),
    ("files", "Files", "copy"),
];

fn items(count: usize, badge: Option<(&str, String)>, disabled: &[&str]) -> Vec<TabBarItem> {
    SECTIONS[..count]
        .iter()
        .map(|(value, label, icon)| {
            let mut item =
                TabBarItem::new(*value, *label, *icon).disabled(disabled.contains(value));
            if let Some((v, b)) = &badge
                && v == value
            {
                item = item.badge(b.clone());
            }
            item
        })
        .collect()
}

/// A phone-sized screen: a title over `content`, with the bar along its bottom.
fn phone(
    title: SharedString,
    content: AnyElement,
    bar: TabBar,
    window: &mut Window,
    cx: &mut App,
) -> AnyElement {
    let theme = current(window, cx);
    let t = theme.tokens();
    div()
        .relative()
        .w(px(402.))
        .h(px(560.))
        .overflow_hidden()
        .rounded(px(24.))
        .border_1()
        .border_color(t.color.border)
        .bg(t.color.background_body)
        .child(
            Stack::vertical()
                .gap(SpacingStep::S3)
                .padding(SpacingStep::S4)
                .child(Text::new(title).text_type(TextType::Large))
                .child(content),
        )
        .child(bar)
        .into_any_element()
}

fn value(
    id: &'static str,
    start: &'static str,
    window: &mut Window,
    cx: &mut App,
) -> Entity<SharedString> {
    window.use_keyed_state(format!("{id}-value"), cx, move |_, _| {
        SharedString::from(start)
    })
}

fn on_change(
    state: &Entity<SharedString>,
) -> impl Fn(SharedString, &mut Window, &mut App) + 'static {
    let state = state.clone();
    move |next, _, cx| {
        state.update(cx, |v, cx| {
            *v = next;
            cx.notify();
        })
    }
}

fn example(title: &'static str, body: AnyElement) -> AnyElement {
    Stack::vertical()
        .gap(SpacingStep::S2)
        .child(Text::new(title).text_type(TextType::Large))
        .child(body)
        .into_any_element()
}

/// Upstream's `Default`: four tabs, a badge on Recent; choosing the selected tab again
/// counts a reselect.
fn default_bar(window: &mut Window, cx: &mut App) -> AnyElement {
    let tab = value("tb-default", "today", window, cx);
    let reselects = window.use_keyed_state("tb-default-reselects", cx, |_, _| 0usize);
    let current_tab = tab.read(cx).clone();
    let count = *reselects.read(cx);
    let title = SECTIONS
        .iter()
        .find(|s| s.0 == current_tab.as_ref())
        .map_or("No tab selected", |s| s.1);
    let bar = TabBar::new("tb-default", current_tab.clone())
        .label("Sections")
        .items(items(4, Some(("recent", "3".into())), &[]))
        .on_change(on_change(&tab))
        .on_reselect({
            let reselects = reselects.clone();
            move |_, _, cx| {
                reselects.update(cx, |n, cx| {
                    *n += 1;
                    cx.notify();
                })
            }
        });
    let content = Text::new(format!("Reselected {count} times")).into_any_element();
    phone(title.into(), content, bar, window, cx)
}

/// Upstream's badges story: badges appear, change and disappear at once, as UIKit's do.
fn badges(window: &mut Window, cx: &mut App) -> AnyElement {
    let tab = value("tb-badges", "today", window, cx);
    let count = window.use_keyed_state("tb-badges-count", cx, |_, _| 0usize);
    let n = *count.read(cx);
    let badge = match n {
        0 => String::new(),
        n if n > 99 => "99+".into(),
        n => n.to_string(),
    };
    let bump = |by: usize| {
        let count = count.clone();
        move |_: &gpui::ClickEvent, _: &mut Window, cx: &mut App| {
            count.update(cx, |n, cx| {
                *n = if by == 0 { 0 } else { *n + by };
                cx.notify();
            })
        }
    };
    let mut tabs = items(4, Some(("recent", badge)), &[]);
    tabs[2] = tabs[2].clone().badge("!");
    let bar = TabBar::new("tb-badges", tab.read(cx).clone())
        .label("Sections")
        .items(tabs)
        .on_change(on_change(&tab));
    let content = Stack::horizontal()
        .gap(SpacingStep::S2)
        .child(Button::new("tb-add-one", "Add one").on_click(bump(1)))
        .child(Button::new("tb-add-ten", "Add ten").on_click(bump(10)))
        .child(Button::new("tb-clear", "Clear").on_click(bump(0)))
        .into_any_element();
    phone("Badges".into(), content, bar, window, cx)
}

/// A disabled tab: it stays in place, dimmed, and is never chosen.
fn disabled(window: &mut Window, cx: &mut App) -> AnyElement {
    let tab = value("tb-disabled", "today", window, cx);
    let bar = TabBar::new("tb-disabled", tab.read(cx).clone())
        .label("Sections")
        .items(items(4, None, &["search"]))
        .on_change(on_change(&tab));
    phone(
        "Search is unavailable".into(),
        Text::new("The arrow keys skip it.").into_any_element(),
        bar,
        window,
        cx,
    )
}

/// Six sections: the bar shows four and More, whose screen lists the rest.
fn more(window: &mut Window, cx: &mut App) -> AnyElement {
    let tab = value("tb-more", "today", window, cx);
    let current_tab = tab.read(cx).clone();
    let tabs = items(6, None, &[]);
    let content = if current_tab.as_ref() == "more" {
        TabBarMoreList::new("tb-more-list", tabs.clone())
            .on_change(on_change(&tab))
            .into_any_element()
    } else {
        Text::new(
            SECTIONS
                .iter()
                .find(|s| s.0 == current_tab.as_ref())
                .map_or("", |s| s.1),
        )
        .into_any_element()
    };
    let bar = TabBar::new("tb-more", current_tab)
        .label("Sections")
        .items(tabs)
        .on_change(on_change(&tab));
    phone("Six sections".into(), content, bar, window, cx)
}

/// The TabBar examples.
pub fn examples(window: &mut Window, cx: &mut App) -> AnyElement {
    Stack::vertical()
        .gap(SpacingStep::S8)
        .child(example("Default", default_bar(window, cx)))
        .child(example("Badges", badges(window, cx)))
        .child(example("Disabled tab", disabled(window, cx)))
        .child(example("More than five tabs", more(window, cx)))
        .into_any_element()
}
