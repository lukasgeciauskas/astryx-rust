//! The TransferList gallery (upstream's `Lab/TransferListSelector` and
//! `Lab/TransferList` stories).

use std::rc::Rc;

use gpui::{
    AnyElement, App, Entity, IntoElement, ParentElement, SharedString, Styled, Window, div, px,
};
use rustryx_components::layout::{SpacingStep, Stack};
use rustryx_components::reorderable::{
    ItemKey, Reorderable, ReorderableOptions, create_reorderable,
};
use rustryx_components::text::{Heading, HeadingLevel};

use super::{CommitBehavior, TransferList, TransferListOption, TransferListSelector, move_value};

fn basic_options() -> Rc<[TransferListOption]> {
    [
        ("name", "Name", "Primary display name for the record"),
        ("owner", "Owner", "Person responsible for the record"),
        ("status", "Status", "Current workflow state"),
        ("priority", "Priority", "Relative urgency"),
        ("team", "Team", "Owning team"),
        ("updated", "Last updated", "Most recent change time"),
        ("created", "Created", "Original creation time"),
    ]
    .into_iter()
    .map(|(value, label, description)| {
        TransferListOption::new(value, label).description(description)
    })
    .collect::<Vec<_>>()
    .into()
}

fn grouped_options() -> Rc<[TransferListOption]> {
    vec![
        TransferListOption::new("name", "Name")
            .description("Required identity field")
            .group("Identity")
            .transfer_disabled(true)
            .disabled_message("Name is required and fixed in position."),
        TransferListOption::new("owner", "Owner")
            .description("Person responsible for the work")
            .group("Identity")
            .transfer_disabled(true)
            .disabled_message("Owner must remain visible but can be reordered."),
        TransferListOption::new("team", "Team")
            .description("Owning team")
            .group("Identity"),
        TransferListOption::new("status", "Status")
            .description("Current workflow state")
            .group("Planning"),
        TransferListOption::new("priority", "Priority")
            .description("Relative urgency")
            .group("Planning"),
        TransferListOption::new("due", "Due date")
            .description("Planned completion date")
            .group("Planning"),
        TransferListOption::new("updated", "Last updated")
            .description("Most recent change time")
            .group("Activity"),
        TransferListOption::new("created", "Created")
            .description("Original creation time")
            .group("Activity"),
    ]
    .into()
}

fn large_pool() -> Rc<[TransferListOption]> {
    (0..200)
        .map(|index| {
            let number = index + 1;
            TransferListOption::new(format!("field-{number}"), format!("Field {number:03}"))
                .description(format!("Configurable field {number}"))
                .group(format!("Group {}", index / 40 + 1))
        })
        .collect::<Vec<_>>()
        .into()
}

/// The value a story keeps across frames.
fn value(
    id: &'static str,
    initial: &[&str],
    window: &mut Window,
    cx: &mut App,
) -> Entity<Vec<SharedString>> {
    let initial: Vec<SharedString> = initial.iter().map(|v| SharedString::from(*v)).collect();
    window.use_keyed_state(id, cx, move |_, _| initial)
}

/// The owner's reorder: each move lands in the value where reported.
fn move_reorderable(
    value: &Entity<Vec<SharedString>>,
    movable: Option<fn(&str) -> bool>,
) -> Reorderable {
    let value = value.clone();
    let mut options = ReorderableOptions::new(move |m, _, cx| {
        let ItemKey::Text(key) = &m.key else {
            return;
        };
        let (key, to) = (key.clone(), m.to);
        value.update(cx, |v, cx| {
            *v = move_value(v, &key, to);
            cx.notify();
        });
    });
    if let Some(movable) = movable {
        options = options.is_reorderable(move |key| match key {
            ItemKey::Text(key) => movable(key),
            _ => true,
        });
    }
    create_reorderable(options)
}

fn setter(
    value: &Entity<Vec<SharedString>>,
) -> impl Fn(Vec<SharedString>, &mut Window, &mut App) + 'static {
    let value = value.clone();
    move |next, _, cx| {
        value.update(cx, |v, cx| {
            *v = next;
            cx.notify();
        })
    }
}

fn story(title: &'static str, body: impl IntoElement) -> AnyElement {
    Stack::vertical()
        .gap(SpacingStep::S4)
        .child(Heading::new(HeadingLevel::H3, title))
        .child(div().w_full().max_w(px(900.)).child(body))
        .into_any_element()
}

fn basic_selector(
    id: &'static str,
    value: &Entity<Vec<SharedString>>,
    cx: &App,
) -> TransferListSelector {
    let v = value.read(cx).clone();
    let count = v.len();
    TransferListSelector::new(id, "Visible fields", basic_options(), v)
        .selected_label("Visible fields")
        .available_label("Available fields")
        .select_all(true)
        .clear(true)
        .on_change(setter(value))
        .trigger_label(format!("{count} visible fields"))
}

/// The TransferList examples: the selector, immediate and staged, grouped with locked
/// rows, unordered, narrow and over a large pool; then the inline list.
pub fn examples(window: &mut Window, cx: &mut App) -> AnyElement {
    let default = value("transfer-default", &["name", "owner", "status"], window, cx);
    let staged = value("transfer-staged", &["name", "owner", "status"], window, cx);
    let grouped = value(
        "transfer-grouped",
        &["name", "owner", "status", "updated"],
        window,
        cx,
    );
    let unordered = value(
        "transfer-unordered",
        &["status", "priority", "team"],
        window,
        cx,
    );
    let narrow = value("transfer-narrow", &["name", "owner", "status"], window, cx);
    let large = value(
        "transfer-large",
        &[
            "field-1", "field-2", "field-3", "field-4", "field-5", "field-6", "field-7", "field-8",
            "field-9", "field-10", "field-11", "field-12",
        ],
        window,
        cx,
    );
    let inline = value("transfer-inline", &["name", "owner", "status"], window, cx);
    let searchable = value("transfer-searchable", &["name", "owner"], window, cx);
    let empty = value("transfer-empty", &[], window, cx);

    let grouped_value = grouped.read(cx).clone();
    let unordered_value = unordered.read(cx).clone();
    let narrow_value = narrow.read(cx).clone();
    let large_value = large.read(cx).clone();
    Stack::vertical()
        .gap(SpacingStep::S8)
        .child(story(
            "Default",
            basic_selector("transfer-default-selector", &default, cx)
                .description("Choose which fields appear. Changes take effect immediately, and drag reorder commits on release.")
                .reorderable(move_reorderable(&default, None)),
        ))
        .child(story(
            "Staged changes",
            basic_selector("transfer-staged-selector", &staged, cx)
                .description("Review the complete field selection before applying it.")
                .reorderable(move_reorderable(&staged, None))
                .commit_behavior(CommitBehavior::Staged),
        ))
        .child(story(
            "Grouped and locked",
            TransferListSelector::new("transfer-grouped-selector", "Record fields", grouped_options(), grouped_value.clone())
                .description("Fields are grouped by purpose. Removal and reordering constraints can be applied independently.")
                .on_change(setter(&grouped))
                // Name and Status keep their place.
                .reorderable(move_reorderable(&grouped, Some(|field| field != "name" && field != "status")))
                .selected_label("Shown")
                .available_label("Hidden")
                .select_all(true)
                .clear(true)
                .trigger_label(format!("{} shown fields", grouped_value.len())),
        ))
        .child(story(
            "Unordered",
            TransferListSelector::new("transfer-unordered-selector", "Included filters", basic_options(), unordered_value.clone())
                .description("Use an unordered transfer list when selection matters but display order does not.")
                .on_change(setter(&unordered))
                .selected_label("Included")
                .available_label("Not included")
                .select_all(true)
                .clear(true)
                .trigger_label(format!("{} included filters", unordered_value.len())),
        ))
        .child(story(
            "Narrow container",
            TransferListSelector::new("transfer-narrow-selector", "Mobile field settings", basic_options(), narrow_value.clone())
                .description("The two panels stack when horizontal space is limited.")
                .on_change(setter(&narrow))
                .trigger_label(format!("{} visible fields", narrow_value.len()))
                .width(px(360.))
                .selected_label("Visible")
                .available_label("Available")
                .select_all(true)
                .clear(true),
        ))
        .child(story(
            "Large pool",
            TransferListSelector::new("transfer-large-selector", "Report fields", large_pool(), large_value.clone())
                .description("Search a pool of 200 options while preserving the chosen display order.")
                .on_change(setter(&large))
                .selected_label("In report")
                .available_label("Available")
                .search(true)
                .search_placeholder("Search 200 fields")
                .select_all(true)
                .clear(true)
                .trigger_label(format!("{} report fields", large_value.len())),
        ))
        .child(story(
            "TransferList",
            TransferList::new("transfer-inline-list", "Visible fields", basic_options(), inline.read(cx).clone())
                .description("Choose and order the fields shown in the table.")
                .on_change(setter(&inline))
                .reorderable(move_reorderable(&inline, None))
                .select_all(true)
                .clear(true),
        ))
        .child(story(
            "TransferList with search",
            TransferList::new("transfer-searchable-list", "Visible fields", basic_options(), searchable.read(cx).clone())
                .on_change(setter(&searchable))
                .search(true)
                .reorderable(move_reorderable(&searchable, None)),
        ))
        .child(story(
            "TransferList, empty",
            TransferList::new("transfer-empty-list", "Visible fields", Vec::<TransferListOption>::new(), empty.read(cx).clone())
                .on_change(setter(&empty))
                .selected_empty_text("Nothing chosen yet")
                .available_empty_text("No fields to choose from"),
        ))
        .into_any_element()
}
