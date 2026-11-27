//! Upstream `TabBar.test.tsx`'s More list cases: the tabs past the fourth in order, the
//! selected row, a disabled row, a badged row's name and the list's name.

use std::cell::RefCell;
use std::rc::Rc;

use gpui::{Context, Modifiers, Render, TestAppContext, VisualTestContext, point, px};
use rustryx_i18n::I18n;

use super::*;

fn six() -> Vec<TabBarItem> {
    ["home", "library", "browse", "profile", "settings", "help"]
        .iter()
        .map(|v| {
            let label = format!("{}{}", v[..1].to_uppercase(), &v[1..]);
            TabBarItem::new(*v, label, "home")
        })
        .collect()
}

type Log = Rc<RefCell<Vec<SharedString>>>;

struct Host {
    items: Vec<TabBarItem>,
    value: Option<&'static str>,
    log: Log,
    i18n: Option<I18n>,
}

impl Render for Host {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let log = self.log.clone();
        let mut list = TabBarMoreList::new("more", self.items.clone())
            .on_change(move |v, _, _| log.borrow_mut().push(v));
        if let Some(v) = self.value {
            list = list.value(v);
        }
        let el = div()
            .w(px(400.))
            .child(div().debug_selector(|| "more-list".into()).child(list));
        match &self.i18n {
            Some(i) => rustryx_i18n::provider(i.clone(), el).into_any_element(),
            None => el.into_any_element(),
        }
    }
}

fn mount<'a>(
    cx: &'a mut TestAppContext,
    items: Vec<TabBarItem>,
    value: Option<&'static str>,
    i18n: Option<I18n>,
) -> (&'a mut VisualTestContext, Log) {
    cx.update(|cx| {
        rustryx_theme::init(cx);
        rustryx_primitives::focus::init(cx);
    });
    let log = Log::default();
    let (_, cx) = cx.add_window_view({
        let log = log.clone();
        move |_, _| Host {
            items,
            value,
            log,
            i18n,
        }
    });
    cx.run_until_parked();
    (cx, log)
}

fn drawn() -> (SharedString, Vec<DrawnRow>) {
    DRAWN.with(|d| d.borrow().clone()).expect("the list drew")
}

/// Clicks row `index` of `rows`.
fn click_row(cx: &mut VisualTestContext, index: usize, rows: usize) {
    let b = cx.debug_bounds("more-list").expect("the list is drawn");
    let row = b.size.height / rows as f32;
    cx.simulate_click(
        point(
            b.origin.x + px(200.),
            b.origin.y + row * (index as f32 + 0.5),
        ),
        Modifiers::none(),
    );
    cx.run_until_parked();
}

#[gpui::test]
fn lists_the_tabs_past_the_fourth_and_a_row_chooses_its_tab(cx: &mut TestAppContext) {
    let (cx, log) = mount(cx, six(), Some("help"), None);
    let (label, rows) = drawn();
    assert_eq!(label.as_ref(), "More");
    let values: Vec<_> = rows.iter().map(|r| r.value.to_string()).collect();
    assert_eq!(values, ["settings", "help"]);
    click_row(cx, 0, 2);
    assert_eq!(*log.borrow(), [SharedString::from("settings")]);
}

#[gpui::test]
fn reads_the_selected_tabs_more_row_as_selected_and_no_other_row(cx: &mut TestAppContext) {
    let _ = mount(cx, six(), Some("help"), None);
    let (_, rows) = drawn();
    assert_eq!(
        rows.iter().map(|r| r.selected).collect::<Vec<_>>(),
        [false, true]
    );
}

#[gpui::test]
fn keeps_a_disabled_tab_past_the_fourth_disabled_in_its_more_row_which_a_tap_never_chooses(
    cx: &mut TestAppContext,
) {
    // spec:AST-085 FR2.
    let mut items = six();
    items[5] = TabBarItem::new("help", "Help", "home").disabled(true);
    let (cx, log) = mount(cx, items, None, None);
    assert!(drawn().1[1].disabled);
    click_row(cx, 1, 2);
    assert!(log.borrow().is_empty());
    click_row(cx, 0, 2);
    assert_eq!(*log.borrow(), [SharedString::from("settings")]);
}

#[gpui::test]
fn shows_a_tabs_badge_in_its_more_row_spoken_with_the_row(cx: &mut TestAppContext) {
    let mut items = six();
    items[5] = TabBarItem::new("help", "Help", "home").badge(2);
    let _ = mount(cx, items, None, None);
    assert_eq!(drawn().1[1].name.as_ref(), "Help, 2");
}

#[gpui::test]
fn names_its_list_with_the_catalog_message_so_a_translation_can_change_it(cx: &mut TestAppContext) {
    let i18n = I18n::new("en").overrides("en", [("@astryx.tabBar.more", "Others")]);
    let _ = mount(cx, six(), None, Some(i18n));
    assert_eq!(drawn().0.as_ref(), "Others");
}

#[gpui::test]
fn five_tabs_or_fewer_list_nothing(cx: &mut TestAppContext) {
    let _ = mount(cx, six()[..5].to_vec(), None, None);
    assert!(drawn().1.is_empty());
}
