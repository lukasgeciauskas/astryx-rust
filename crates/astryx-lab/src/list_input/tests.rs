//! Upstream `ListInput.test.tsx`, the parts a GPUI window can drive.

use std::cell::RefCell;
use std::rc::Rc;

use astryx_components::field::{InputStatus, StatusPlacement};
use astryx_components::reorderable::{ItemKey, ReorderableOptions, create_reorderable};
use astryx_primitives::announce::{Politeness, announcement};
use gpui::prelude::*;
use gpui::{
    App, Context, Entity, Modifiers, Render, SharedString, TestAppContext, VisualTestContext,
    Window, div, px,
};

use super::*;

#[derive(Clone, Debug, PartialEq)]
struct Guest {
    id: SharedString,
    name: SharedString,
}

fn guest(id: &str, name: &str) -> Guest {
    Guest {
        id: id.to_string().into(),
        name: name.to_string().into(),
    }
}

/// What the renderers saw, per cell: the label, the status, disabled and the id.
type Seen = Rc<RefCell<Vec<(SharedString, Option<InputStatus>, bool, StatusPlacement)>>>;
type Changes = Rc<RefCell<Vec<ListInputChange<Guest>>>>;

#[derive(Default)]
struct Options {
    disabled: bool,
    loading: bool,
    max_items: Option<usize>,
    reorder: Option<astryx_components::reorderable::Reorderable>,
    statuses: bool,
}

struct Host {
    value: Entity<Vec<Guest>>,
    changes: Changes,
    seen: Seen,
    options: Rc<Options>,
}

impl Render for Host {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let seen = self.seen.clone();
        seen.borrow_mut().clear();
        let columns = vec![ListInputColumn::new(
            "name",
            "Name",
            move |ctx: &ListInputRenderContext<Guest>, _: &mut Window, _: &mut App| {
                seen.borrow_mut().push((
                    ctx.label.clone(),
                    ctx.status.clone(),
                    ctx.is_disabled,
                    ctx.status_placement,
                ));
                div().h(px(32.)).w_full().into_any_element()
            },
        )];
        let (value, changes) = (self.value.clone(), self.changes.clone());
        let counter = Rc::new(RefCell::new(0));
        let mut input = ListInput::new(
            "guests",
            "Guests",
            self.value.read(cx).clone(),
            |g: &Guest| g.id.clone(),
            move || {
                *counter.borrow_mut() += 1;
                guest(&format!("new-{}", counter.borrow()), "")
            },
            columns,
        )
        .item_name("guest")
        .disabled(self.options.disabled)
        .loading(self.options.loading)
        .on_change(move |next, change, _, cx| {
            changes.borrow_mut().push(change);
            value.update(cx, |v, cx| {
                *v = next;
                cx.notify();
            });
        });
        if let Some(max) = self.options.max_items {
            input = input.max_items(max);
        }
        if let Some(r) = self.options.reorder.clone() {
            input = input.reorderable(r);
        }
        if self.options.statuses {
            input = input
                .field_status(|g, column, _| {
                    (column == "name" && g.name.is_empty())
                        .then(|| InputStatus::error("Name is required"))
                })
                .item_status(|g, _| {
                    (g.id == "b").then(|| InputStatus::warning("Check this guest"))
                });
        }
        div().w(px(800.)).child(input)
    }
}

struct Rig<'a> {
    cx: &'a mut VisualTestContext,
    value: Entity<Vec<Guest>>,
    changes: Changes,
    seen: Seen,
}

fn rig(cx: &mut TestAppContext, initial: Vec<Guest>, options: Options) -> Rig<'_> {
    cx.update(|cx| {
        astryx_theme::init(cx);
        astryx_primitives::focus::init(cx);
        astryx_primitives::editing::init(cx);
    });
    let value = cx.update(|cx| cx.new(|_| initial));
    let changes: Changes = Rc::default();
    let seen: Seen = Rc::default();
    let (v, c, s) = (value.clone(), changes.clone(), seen.clone());
    let options = Rc::new(options);
    let (_, cx) = cx.add_window_view(move |_, _| Host {
        value: v,
        changes: c,
        seen: s,
        options,
    });
    cx.update(|window, _| window.activate_window());
    let mut r = Rig {
        cx,
        value,
        changes,
        seen,
    };
    r.settle();
    r
}

impl Rig<'_> {
    fn settle(&mut self) {
        for _ in 0..4 {
            self.cx.update(|window, _| window.refresh());
            self.cx.run_until_parked();
        }
        self.cx
            .executor()
            .advance_clock(std::time::Duration::from_millis(400));
        self.cx.run_until_parked();
    }

    fn click(&mut self, selector: &str) {
        self.settle();
        let b = self
            .cx
            .debug_bounds(Box::leak(selector.to_string().into_boxed_str()))
            .unwrap_or_else(|| panic!("{selector} is drawn"));
        self.cx.simulate_click(b.center(), Modifiers::none());
        self.settle();
    }

    /// Hovers a record, which reveals its Remove, then clicks Remove.
    fn remove(&mut self, key: &str) {
        self.settle();
        let row = self
            .cx
            .debug_bounds(Box::leak(format!("guests-record-{key}").into_boxed_str()))
            .unwrap_or_else(|| panic!("record {key} is drawn"));
        self.cx
            .simulate_mouse_move(row.center(), None, Modifiers::none());
        self.settle();
        self.click(&format!("guests-remove-{key}"));
    }

    fn ids(&mut self) -> Vec<SharedString> {
        self.value
            .read_with(self.cx, |v, _| v.iter().map(|g| g.id.clone()).collect())
    }
}

fn three() -> Vec<Guest> {
    vec![guest("a", "Ada"), guest("b", "Bo"), guest("c", "Cy")]
}

#[gpui::test]
fn emits_add_and_remove_changes_with_the_affected_item_and_index(cx: &mut TestAppContext) {
    let mut r = rig(cx, three(), Options::default());
    r.click("guests-add");
    assert_eq!(r.ids(), ["a", "b", "c", "new-1"]);
    assert_eq!(
        r.changes.borrow()[0],
        ListInputChange::Add {
            item: guest("new-1", ""),
            index: 3
        }
    );
    let said =
        r.cx.update(|window, cx| announcement(Politeness::Polite, window, cx));
    assert!(said.contains('4'), "{said}");
    r.remove("b");
    assert_eq!(r.ids(), ["a", "c", "new-1"]);
    assert_eq!(
        r.changes.borrow()[1],
        ListInputChange::Remove {
            item: guest("b", "Bo"),
            index: 1
        }
    );
}

#[gpui::test]
fn shows_the_empty_state_and_keeps_add_available(cx: &mut TestAppContext) {
    let mut r = rig(cx, Vec::new(), Options::default());
    r.click("guests-add");
    assert_eq!(r.ids(), ["new-1"]);
}

#[gpui::test]
fn names_each_cell_by_its_column_and_its_position_past_the_first(cx: &mut TestAppContext) {
    let r = rig(cx, three(), Options::default());
    let labels: Vec<SharedString> = r.seen.borrow().iter().map(|s| s.0.clone()).collect();
    let i18n = astryx_i18n::I18n::new("en-US");
    let second = i18n.format(
        "@astryx.listInput.fieldLabelWithPosition",
        &[
            ("header", Arg::from("Name")),
            ("itemName", Arg::from("guest")),
            ("position", Arg::Num(2.)),
            ("total", Arg::Num(3.)),
        ],
    );
    assert_eq!(labels[0], "Name");
    assert_eq!(labels[1], second);
    assert!(
        r.seen
            .borrow()
            .iter()
            .all(|s| s.3 == StatusPlacement::Tooltip && !s.2)
    );
}

#[gpui::test]
fn passes_field_statuses_to_the_renderer(cx: &mut TestAppContext) {
    let r = rig(
        cx,
        vec![guest("a", ""), guest("b", "Bo")],
        Options {
            statuses: true,
            ..Options::default()
        },
    );
    let seen = r.seen.borrow();
    assert_eq!(seen[0].1, Some(InputStatus::error("Name is required")));
    assert_eq!(seen[1].1, None);
}

#[gpui::test]
fn disables_add_at_max_items_while_keeping_removal(cx: &mut TestAppContext) {
    let mut r = rig(
        cx,
        three(),
        Options {
            max_items: Some(3),
            ..Options::default()
        },
    );
    r.click("guests-add");
    assert_eq!(r.ids(), ["a", "b", "c"]);
    r.remove("a");
    assert_eq!(r.ids(), ["b", "c"]);
}

#[gpui::test]
fn locks_every_field_and_mutation_when_disabled_or_loading(cx: &mut TestAppContext) {
    for options in [
        Options {
            disabled: true,
            ..Options::default()
        },
        Options {
            loading: true,
            ..Options::default()
        },
    ] {
        let mut r = rig(cx, three(), options);
        assert!(
            r.seen.borrow().iter().all(|s| s.2),
            "the fields are disabled"
        );
        r.remove("a");
        assert_eq!(r.ids(), ["a", "b", "c"]);
        assert!(r.changes.borrow().is_empty());
        r.cx.update(|window, _| window.remove_window());
    }
}

#[gpui::test]
fn moves_a_record_in_reorder_mode_through_the_owner(cx: &mut TestAppContext) {
    let moves = Rc::new(RefCell::new(Vec::new()));
    let m = moves.clone();
    let reorderable = create_reorderable(ReorderableOptions::new(move |mv, _, _| {
        m.borrow_mut().push((mv.key.clone(), mv.to))
    }));
    let mut r = rig(
        cx,
        three(),
        Options {
            reorder: Some(reorderable.clone()),
            ..Options::default()
        },
    );
    r.cx.update(|window, cx| reorderable.start(ItemKey::Text("a".into()), window, cx));
    r.settle();
    r.cx.simulate_keystrokes("down");
    r.settle();
    assert_eq!(*moves.borrow(), [(ItemKey::Text("a".into()), 1)]);
}
