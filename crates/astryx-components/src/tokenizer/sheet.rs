//! Tokenizer's bottom-sheet presentation, upstream
//! `Tokenizer/TokenizerBottomSheet.tsx` (FR16-FR27).
//!
//! - The closed field is one button showing the tokens (display-only) or the
//!   placeholder; it opens a tall modal BottomSheet (FR17).
//! - The sheet edits a draft that starts as `value`: checkbox rows toggle
//!   it, a Create row adds to it, Clear all empties it (after asking); Save
//!   reports it once with the last edit that still holds (FR19, FR22, FR24,
//!   FR26). Dismissing a changed draft asks first (FR27).
//! - Rows keep their places while it is open: the items chosen at open, then
//!   other draft items the results lack, then the results (FR19). Long lists
//!   render 50 rows at a time (FR21).

use std::rc::Rc;
use std::time::Duration;

use astryx_i18n::{Arg, current as i18n};
use astryx_primitives::editing::{EditorStyle, FieldSettings, TextEditState, text_editor};
use astryx_primitives::{ElementSize, Politeness, announce, prefers_reduced_motion};
use astryx_theme::current;
use gpui::{
    AnyElement, App, ElementId, Entity, Focusable, IntoElement, ParentElement, Pixels, RenderOnce,
    Role, ScrollHandle, SharedString, Styled, Window, canvas, div, prelude::*, px, relative,
};

use super::field::{TokenizerChange, TokenizerChangeKind};
use crate::button::{Button, ButtonVariant};
use crate::icon::{Icon, IconColor, IconSize};
use crate::indicator::CheckboxIndicator;
use crate::overlay::{BottomSheet, Dialog, DialogPurpose, SheetHeight};
use crate::search::{SearchSource, Searchable};
use crate::status::{Spinner, SpinnerSize};
use crate::typeahead::engine::{Engine, Search, engine_of};

/// Rows rendered at a time (upstream `LIST_RENDER_BATCH_SIZE`).
pub(crate) const BATCH: usize = 50;

type CommitFn<T> = Rc<dyn Fn(Vec<T>, TokenizerChange<T>, &mut Window, &mut App)>;
type CreateFn<T> = Rc<dyn Fn(&str) -> T>;
type QueryFn = Rc<dyn Fn(SharedString, &mut Window, &mut App)>;
type ActFn = Rc<dyn Fn(&mut Window, &mut App)>;
type ToggleFn<T> = Rc<dyn Fn(T, &mut Window, &mut App)>;

/// One open sheet's draft.
pub(crate) struct Draft<T> {
    pub open: bool,
    /// The selection at open, in `value` order.
    pub at_open: Vec<T>,
    pub items: Vec<T>,
    /// The edits, in order: (item, added, created).
    pub edits: Vec<(T, bool, bool)>,
    pub batches: usize,
    pub confirm: Option<Confirm>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Confirm {
    ClearAll,
    Discard,
}

impl<T> Default for Draft<T> {
    fn default() -> Self {
        Draft {
            open: false,
            at_open: Vec::new(),
            items: Vec::new(),
            edits: Vec::new(),
            batches: 1,
            confirm: None,
        }
    }
}

/// FR26: the last edit that still holds, or what the draft differs by.
pub(crate) fn save_change<T: Searchable>(draft: &Draft<T>) -> Option<TokenizerChange<T>> {
    let opened = |i: &T| draft.at_open.iter().any(|o| o.id() == i.id());
    let drafted = |i: &T| draft.items.iter().any(|d| d.id() == i.id());
    let same = draft.items.len() == draft.at_open.len() && draft.items.iter().all(opened);
    if same {
        return None;
    }
    if let Some((item, _, created)) = draft.edits.last() {
        if drafted(item) && !opened(item) {
            let kind = if *created {
                TokenizerChangeKind::Create
            } else {
                TokenizerChangeKind::Add
            };
            return Some(TokenizerChange {
                item: item.clone(),
                kind,
            });
        }
        if opened(item) && !drafted(item) {
            return Some(TokenizerChange {
                item: item.clone(),
                kind: TokenizerChangeKind::Remove,
            });
        }
        let last_removed = !draft.edits.last().is_some_and(|e| e.1);
        if last_removed && let Some(gone) = draft.at_open.iter().find(|o| !drafted(o)) {
            return Some(TokenizerChange {
                item: gone.clone(),
                kind: TokenizerChangeKind::Remove,
            });
        }
    }
    if let Some(added) = draft.items.iter().find(|i| !opened(i)) {
        let created = draft
            .edits
            .iter()
            .any(|(e, a, c)| *a && *c && e.id() == added.id());
        let kind = if created {
            TokenizerChangeKind::Create
        } else {
            TokenizerChangeKind::Add
        };
        return Some(TokenizerChange {
            item: added.clone(),
            kind,
        });
    }
    draft
        .at_open
        .iter()
        .find(|o| !drafted(o))
        .map(|gone| TokenizerChange {
            item: gone.clone(),
            kind: TokenizerChangeKind::Remove,
        })
}

/// What the sheet reads in one frame.
pub(crate) struct SheetProps<T: Searchable> {
    pub id: ElementId,
    pub label: SharedString,
    pub source: Rc<dyn SearchSource<T>>,
    pub max_entries: Option<usize>,
    pub create: Option<CreateFn<T>>,
    pub max_menu_items: usize,
    pub min_query_length: usize,
    pub debounce: Duration,
    pub empty_search_text: Option<SharedString>,
    pub on_change: Option<CommitFn<T>>,
    pub on_change_query: Option<QueryFn>,
    pub disabled: bool,
}

pub(crate) fn draft_state<T: Searchable>(
    id: &ElementId,
    window: &mut Window,
    cx: &mut App,
) -> Entity<Draft<T>> {
    window.use_keyed_state(
        ElementId::Name(format!("{id}-sheet-draft").into()),
        cx,
        |_, _| Draft::default(),
    )
}

/// Opens the sheet: a fresh draft and the bootstrap list.
pub(crate) fn open_sheet<T: Searchable>(
    draft: &Entity<Draft<T>>,
    value: &[T],
    window: &mut Window,
    cx: &mut App,
) {
    draft.update(cx, |d, _| {
        d.open = true;
        d.at_open = value.to_vec();
        d.items = value.to_vec();
        d.edits.clear();
        d.batches = 1;
        d.confirm = None;
    });
    window.refresh();
}

/// The sheet (and its confirmations), while open.
pub(crate) fn sheet<T: Searchable>(
    props: SheetProps<T>,
    window: &mut Window,
    cx: &mut App,
) -> AnyElement {
    let theme = current(window, cx);
    let t = theme.tokens();
    let rem = window.rem_size();
    let t18 = i18n(cx);
    let id = props.id.clone();
    let draft = draft_state::<T>(&id, window, cx);
    let engine: Entity<Engine<T>> =
        engine_of(&ElementId::Name(format!("{id}-sheet").into()), window, cx);
    let editor = window.use_keyed_state(
        ElementId::Name(format!("{id}-sheet-input").into()),
        cx,
        |window, cx| TextEditState::reporting("", false, window, cx),
    );
    let scroll = window
        .use_keyed_state(
            ElementId::Name(format!("{id}-sheet-scroll").into()),
            cx,
            |_, _| ScrollHandle::new(),
        )
        .read(cx)
        .clone();
    let search = Rc::new(Search {
        engine: engine.clone(),
        source: props.source.clone(),
        max_items: props.max_menu_items,
        min_query_length: props.min_query_length,
        debounce: props.debounce,
        entries_on_focus: true,
        exclude: Vec::new(),
        query_entries: None,
        entry_label: None,
    });
    let open = draft.read(cx).open;
    // FR17: a field that becomes disabled closes its sheet, reporting nothing.
    if open && props.disabled {
        draft.update(cx, |d, _| d.open = false);
    }
    // The bootstrap list on open (FR20).
    let opening = window.use_keyed_state(
        ElementId::Name(format!("{id}-sheet-was-open").into()),
        cx,
        |_, _| false,
    );
    if open && !*opening.read(cx) {
        opening.update(cx, |o, _| *o = true);
        let search = search.clone();
        window.defer(cx, move |window, cx| {
            search.engine.update(cx, |e, _| e.query.clear());
            search.bootstrap(window, cx);
        });
    } else if !open && *opening.read(cx) {
        opening.update(cx, |o, _| *o = false);
    }

    // Closing: Save, a clean dismissal, a confirmed discard (FR18).
    let close: ActFn = {
        let (draft, editor, engine, report) = (
            draft.clone(),
            editor.clone(),
            engine.clone(),
            props.on_change_query.clone(),
        );
        Rc::new(move |window, cx| {
            draft.update(cx, |d, _| {
                d.open = false;
                d.confirm = None;
            });
            editor.update(cx, |e, cx| e.set_text("", cx));
            engine.update(cx, |e, _| e.query.clear());
            if let Some(f) = &report {
                f(SharedString::default(), window, cx);
            }
            window.refresh();
        })
    };
    let save: ActFn = {
        let (draft, close, report) = (draft.clone(), close.clone(), props.on_change.clone());
        let max = props.max_entries;
        Rc::new(move |window, cx| {
            if max.is_some_and(|m| draft.read(cx).items.len() > m) {
                return;
            }
            let change = save_change(draft.read(cx));
            let items = draft.read(cx).items.clone();
            close(window, cx);
            if let (Some(change), Some(f)) = (change, &report) {
                f(items, change, window, cx);
            }
        })
    };
    let dismiss: ActFn = {
        let (draft, close) = (draft.clone(), close.clone());
        Rc::new(move |window, cx| {
            let changed = save_change(draft.read(cx)).is_some();
            if changed {
                draft.update(cx, |d, _| d.confirm = Some(Confirm::Discard));
                window.refresh();
            } else {
                close(window, cx);
            }
        })
    };

    // The search field.
    {
        let (search, report) = (search.clone(), props.on_change_query.clone());
        let draft = draft.clone();
        editor.update(cx, |e, _| {
            e.set_settings(FieldSettings {
                spell_check: false,
                ..FieldSettings::default()
            });
            e.on_change = Some(Rc::new(
                move |text: String, window: &mut Window, cx: &mut App| {
                    if let Some(f) = &report {
                        f(text.clone().into(), window, cx);
                    }
                    draft.update(cx, |d, _| d.batches = 1);
                    search.query_changed(text, true, window, cx);
                },
            ));
        });
    }
    let search_label = t18.t("@astryx.tokenizer.search");
    let field = text_editor(
        "sheet-search",
        &editor,
        EditorStyle {
            text_color: t.color.text_primary,
            placeholder: Some(t18.t("@astryx.tokenizer.searchPlaceholder")),
            placeholder_color: t.color.text_secondary,
            selection_color: crate::input::selection_color(t),
            caret_color: t.color.text_primary,
            multiline: false,
            min_lines: 1,
            max_lines: Some(1),
            height: None,
            reduced_motion: prefers_reduced_motion(cx),
            spelling_color: t.color.error,
        },
        cx,
    )
    .flex_1()
    .min_w_0()
    .text_size(t.text.body.size)
    .line_height(relative(t.text.body.leading))
    .role(Role::SearchInput)
    .aria_label(search_label);
    let search_row = div().pb(t.spacing.s2).child(
        div()
            .flex()
            .items_center()
            .gap(t.spacing.s2)
            .min_h(t.size.element_lg)
            .py(t.spacing.s1)
            .px(t.spacing.s2)
            .border(t.border.width)
            .border_color(t.color.border_emphasized)
            .rounded(t.radius.element)
            .bg(t.color.background_surface)
            .child(
                Icon::new("search")
                    .size(IconSize::Sm)
                    .color(IconColor::Secondary),
            )
            .child(field),
    );

    // The rows (FR19): chosen at open, other draft items, then results.
    let (query, results, loading, list_query, has_searched) = {
        let e = engine.read(cx);
        (
            e.query.clone(),
            e.results.clone(),
            e.loading,
            e.list_query.clone(),
            e.has_searched,
        )
    };
    let lower = query.trim().to_lowercase();
    let matches = |item: &T| lower.is_empty() || item.label().to_lowercase().contains(&lower);
    let (at_open, items) = {
        let d = draft.read(cx);
        (d.at_open.clone(), d.items.clone())
    };
    let mut rows: Vec<T> = at_open.iter().filter(|i| matches(i)).cloned().collect();
    for item in items.iter().filter(|i| matches(i)) {
        if !rows.iter().any(|r| r.id() == item.id()) && !results.iter().any(|r| r.id() == item.id())
        {
            rows.push(item.clone());
        }
    }
    let searched = !query.trim().is_empty()
        && !crate::typeahead::engine::below_min(&query, props.min_query_length);
    let pending = loading || (searched && list_query.as_deref() != Some(query.as_str()));
    for item in &results {
        if !rows.iter().any(|r| r.id() == item.id()) {
            rows.push(item.clone());
        }
    }
    let at_limit = props.max_entries.is_some_and(|m| items.len() >= m);
    let over_limit = props.max_entries.is_some_and(|m| items.len() > m);
    // FR22: a Create row for unmatched text.
    let create_text = query.trim().to_string();
    let offers_create = props.create.is_some()
        && !create_text.is_empty()
        && !at_limit
        && !items
            .iter()
            .any(|i| i.id().as_ref() == create_text || i.label().to_lowercase() == lower)
        && !results.iter().any(|r| r.label().to_lowercase() == lower);
    let toggle: ToggleFn<T> = {
        let draft = draft.clone();
        let max = props.max_entries;
        Rc::new(move |item: T, window: &mut Window, cx: &mut App| {
            let chosen = draft.read(cx).items.iter().any(|d| d.id() == item.id());
            if !chosen && max.is_some_and(|m| draft.read(cx).items.len() >= m) {
                return;
            }
            draft.update(cx, |d, _| {
                if chosen {
                    d.items.retain(|d| d.id() != item.id());
                } else {
                    d.items.push(item.clone());
                }
                d.edits.push((item.clone(), !chosen, false));
            });
            let key = if chosen {
                "@astryx.tokenizer.tokenRemoved"
            } else {
                "@astryx.tokenizer.tokenAdded"
            };
            let text = i18n(cx).format(key, &[("label", item.label().as_ref().into())]);
            announce(text, Politeness::Polite, window, cx);
            window.refresh();
        })
    };
    let total = rows.len();
    let batches = draft.read(cx).batches;
    let shown = (batches * BATCH).min(total);
    let mut list_rows: Vec<AnyElement> = Vec::new();
    if offers_create && let Some(factory) = props.create.clone() {
        let (draft, editor, engine) = (draft.clone(), editor.clone(), engine.clone());
        let text = create_text.clone();
        let add_label = t18.format(
            "@astryx.tokenizer.addValue",
            &[("value", text.as_str().into())],
        );
        list_rows.push(
            div()
                .debug_selector({
                    let s = format!("{id}-sheet-create");
                    move || s
                })
                .flex()
                .items_center()
                .gap(t.spacing.s2)
                .w_full()
                .min_h(px(44.))
                .py(t.spacing.s2)
                .px(t.spacing.s1)
                .child(div().flex_1().min_w_0().child(t18.format(
                    "@astryx.tokenizer.create",
                    &[("value", text.as_str().into())],
                )))
                .child(
                    Button::new(ElementId::Name(format!("{id}-sheet-add").into()), add_label)
                        .size(ElementSize::Sm)
                        .on_click(move |_, window, cx| {
                            let item = factory(&text);
                            draft.update(cx, |d, _| {
                                d.items.push(item.clone());
                                d.edits.push((item.clone(), true, true));
                            });
                            let said = i18n(cx).format(
                                "@astryx.tokenizer.tokenAdded",
                                &[("label", item.label().as_ref().into())],
                            );
                            announce(said, Politeness::Polite, window, cx);
                            editor.update(cx, |e, cx| e.set_text("", cx));
                            engine.update(cx, |e, _| e.query.clear());
                            window.focus(&editor.read(cx).focus_handle(cx), cx);
                            window.refresh();
                        }),
                )
                .into_any_element(),
        );
    }
    for (at, item) in rows.iter().take(shown).enumerate() {
        let checked = items.iter().any(|d| d.id() == item.id());
        let inert = !checked && at_limit;
        let (toggle, item_c) = (toggle.clone(), item.clone());
        let selector = format!("{id}-sheet-row-{}", item.id());
        list_rows.push(
            div()
                .id(ElementId::Name(format!("{id}-sheet-row-{at}").into()))
                .debug_selector(move || selector)
                .flex()
                .items_center()
                .justify_between()
                .gap(t.spacing.s2)
                .w_full()
                .min_h(px(44.))
                .py(t.spacing.s2)
                .px(t.spacing.s1)
                .rounded(t.radius.element)
                .text_color(t.color.text_primary)
                .role(Role::CheckBox)
                .aria_label(item.label())
                .aria_toggled(if checked {
                    gpui::Toggled::True
                } else {
                    gpui::Toggled::False
                })
                .aria_disabled(inert)
                .aria_position_in_set(at + 1)
                .aria_size_of_set(total)
                .when(!inert, |el| el.cursor_pointer())
                .on_click(move |_, window, cx| {
                    if !inert {
                        toggle(item_c.clone(), window, cx)
                    }
                })
                .child(div().flex_1().min_w_0().child(item.label()))
                .child(CheckboxIndicator::new(checked).disabled(inert))
                .into_any_element(),
        );
    }
    let empty = rows.is_empty() && !offers_create && has_searched && searched && !pending;
    let list = div()
        .id(ElementId::Name(format!("{id}-sheet-list").into()))
        .debug_selector({
            let s = format!("{id}-sheet-list");
            move || s
        })
        .flex()
        .flex_col()
        .flex_1()
        .min_h_0()
        .overflow_y_scroll()
        .track_scroll(&scroll)
        .role(Role::List)
        .aria_busy(pending)
        .when(pending, |el| {
            el.child(
                div()
                    .flex()
                    .items_center()
                    .justify_center()
                    .min_h(px(44.))
                    .child(
                        Spinner::new(ElementId::Name(format!("{id}-sheet-spinner").into()))
                            .size(SpinnerSize::Sm),
                    ),
            )
        })
        .when(empty, |el| {
            el.child(
                div()
                    .debug_selector({
                        let s = format!("{id}-sheet-empty");
                        move || s
                    })
                    .p(t.spacing.s4)
                    .text_center()
                    .text_color(t.color.text_secondary)
                    .child(
                        props
                            .empty_search_text
                            .clone()
                            .unwrap_or_else(|| t18.t("@astryx.typeahead.emptySearchResults")),
                    ),
            )
        })
        .children(list_rows)
        // FR21: the next batch when the list nears its end.
        .child({
            let (draft, scroll) = (draft.clone(), scroll.clone());
            canvas(
                |_, _, _| {},
                move |_, _, window, cx| {
                    let bounds = scroll.bounds();
                    let max = scroll.max_offset().y;
                    let offset = -scroll.offset().y;
                    if shown < total && (max - offset) < px(200.) && bounds.size.height > px(0.) {
                        let draft = draft.clone();
                        window.defer(cx, move |window, cx| {
                            draft.update(cx, |d, _| d.batches += 1);
                            window.refresh();
                        });
                    }
                },
            )
            .absolute()
            .size_0()
        });

    // The footer, pinned (FR18): Clear all and Save.
    let clear_all = {
        let draft = draft.clone();
        let nothing = items.is_empty();
        let mut b = Button::new(
            ElementId::Name(format!("{id}-sheet-clear").into()),
            t18.t("@astryx.tokenizer.clearAll"),
        )
        .variant(ButtonVariant::Secondary)
        .size(ElementSize::Lg)
        .disabled(nothing)
        .on_click(move |_, window, cx| {
            if !draft.read(cx).items.is_empty() {
                draft.update(cx, |d, _| d.confirm = Some(Confirm::ClearAll));
                window.refresh();
            }
        });
        if nothing {
            b = b.tooltip(t18.t("@astryx.tokenizer.nothingToClear"));
        }
        div()
            .debug_selector({
                let s = format!("{id}-sheet-clear");
                move || s
            })
            .flex_1()
            .child(b.w_full())
    };
    let save_button = {
        let save = save.clone();
        let mut b = Button::new(
            ElementId::Name(format!("{id}-sheet-save").into()),
            t18.t("@astryx.tokenizer.save"),
        )
        .variant(ButtonVariant::Primary)
        .size(ElementSize::Lg)
        .disabled(over_limit)
        .on_click(move |_, window, cx| save(window, cx));
        if over_limit {
            b = b.tooltip(t18.format(
                "@astryx.tokenizer.selectionLimit",
                &[("count", Arg::Num(props.max_entries.unwrap_or(0) as f64))],
            ));
        }
        div()
            .debug_selector({
                let s = format!("{id}-sheet-save");
                move || s
            })
            .flex_1()
            .child(b.w_full())
    };
    let footer = div()
        .flex()
        .items_center()
        .gap(t.spacing.s2)
        .pt(t.spacing.s3)
        .child(clear_all)
        .child(save_button);
    let body = div()
        .flex()
        .flex_col()
        .h_full()
        .min_h_0()
        .px(t.spacing.s3)
        .pt(t.spacing.s2)
        .pb(t.spacing.s3)
        .child(search_row)
        .child(list)
        .child(footer);

    // The confirmations (FR24, FR27).
    let confirm = draft.read(cx).confirm;
    let confirmation = confirm.map(|which| {
        let (title, description, destructive, safe) = match which {
            Confirm::ClearAll => (
                t18.t("@astryx.tokenizer.clearAllTitle"),
                t18.format(
                    "@astryx.tokenizer.clearAllDescription",
                    &[("label", props.label.as_ref().into())],
                ),
                t18.t("@astryx.tokenizer.clearSelection"),
                t18.t("@astryx.button.cancel"),
            ),
            Confirm::Discard => (
                t18.t("@astryx.tokenizer.discardChangesTitle"),
                t18.format(
                    "@astryx.tokenizer.discardChangesDescription",
                    &[("label", props.label.as_ref().into())],
                ),
                t18.t("@astryx.tokenizer.discardChanges"),
                t18.t("@astryx.tokenizer.keepEditing"),
            ),
        };
        let safe_action: ActFn = {
            let (draft, editor) = (draft.clone(), editor.clone());
            Rc::new(move |window, cx| {
                draft.update(cx, |d, _| d.confirm = None);
                if which == Confirm::Discard {
                    window.focus(&editor.read(cx).focus_handle(cx), cx);
                }
                window.refresh();
            })
        };
        let destructive_action: ActFn = {
            let (draft, close) = (draft.clone(), close.clone());
            Rc::new(move |window, cx| match which {
                Confirm::ClearAll => {
                    draft.update(cx, |d, _| {
                        if let Some(last) = d.items.last().cloned() {
                            d.edits.push((last, false, false));
                        }
                        d.items.clear();
                        d.confirm = None;
                    });
                    announce(
                        i18n(cx).t("@astryx.tokenizer.allRemoved"),
                        Politeness::Polite,
                        window,
                        cx,
                    );
                    window.refresh();
                }
                Confirm::Discard => close(window, cx),
            })
        };
        let (safe_a, safe_b, destroy) = (
            safe_action.clone(),
            safe_action.clone(),
            destructive_action.clone(),
        );
        Dialog::new(ElementId::Name(format!("{id}-sheet-confirm").into()), true)
            .purpose(DialogPurpose::Info)
            .label(title.clone())
            .on_open_change(move |open, window, cx| {
                if !open {
                    safe_a(window, cx)
                }
            })
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(t.spacing.s3)
                    .child(crate::text::Heading::new(
                        crate::text::HeadingLevel::H3,
                        title,
                    ))
                    .child(div().text_color(t.color.text_secondary).child(description))
                    .child(
                        div()
                            .id(ElementId::Name(format!("{id}-sheet-actions").into()))
                            .flex()
                            .flex_col()
                            .gap(t.spacing.s2)
                            .role(Role::Group)
                            .child(
                                div()
                                    .debug_selector({
                                        let s = format!("{id}-sheet-destroy");
                                        move || s
                                    })
                                    .child(
                                        Button::new(
                                            ElementId::Name(format!("{id}-sheet-destroy").into()),
                                            destructive,
                                        )
                                        .variant(ButtonVariant::Destructive)
                                        .size(ElementSize::Lg)
                                        .w_full()
                                        .on_click(move |_, window, cx| destroy(window, cx)),
                                    ),
                            )
                            .child(
                                Button::new(
                                    ElementId::Name(format!("{id}-sheet-safe").into()),
                                    safe,
                                )
                                .size(ElementSize::Lg)
                                .w_full()
                                .on_click(move |_, window, cx| safe_b(window, cx)),
                            ),
                    ),
            )
            .into_any_element()
    });

    let on_sheet_change = dismiss.clone();
    let mut bottom = BottomSheet::new(
        ElementId::Name(format!("{id}-sheet").into()),
        open,
        props.label.clone(),
    )
    .height(SheetHeight::Tall)
    .on_open_change(move |open, window, cx| {
        if !open {
            on_sheet_change(window, cx)
        }
    });
    if open {
        bottom = bottom.child(body);
    }
    let _ = (rem, search, Pixels::default());
    div()
        .display_contents()
        .child(bottom)
        .children(confirmation)
        .into_any_element()
}

/// The closed sheet field's trigger (FR17): the tokens, display-only, or the
/// placeholder.
#[derive(IntoElement)]
pub(crate) struct SheetTrigger {
    pub id: ElementId,
    pub label: SharedString,
    pub content: AnyElement,
    pub disabled: bool,
    pub open: bool,
    pub on_open: ActFn,
}

impl RenderOnce for SheetTrigger {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let focus = astryx_primitives::control_focus(&self.id, window, cx);
        let theme = current(window, cx);
        let t = theme.tokens();
        let rem = window.rem_size();
        let open = self.on_open.clone();
        let mut el = div()
            .id(self.id.clone())
            .debug_selector({
                let s = format!("{}", self.id);
                move || s
            })
            .relative()
            .flex_1()
            .min_w_0()
            .track_focus(&focus)
            .role(Role::Button)
            .aria_label(self.label)
            .aria_has_popup(gpui::HasPopup::Dialog)
            .aria_expanded(self.open)
            .aria_disabled(self.disabled)
            .child(self.content);
        if !self.disabled {
            el = el
                .cursor_pointer()
                .on_click(move |_, window, cx| open(window, cx));
        }
        if astryx_primitives::is_focus_visible(&focus, window) {
            el = el.child(crate::focus::focus_ring(
                t,
                t.radius.element.to_pixels(rem),
                rem,
            ));
        }
        el
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::search::SearchItem;

    fn item(id: &str) -> SearchItem {
        SearchItem::new(id.to_string(), id.to_string())
    }

    fn draft(at_open: &[&str], items: &[&str], edits: &[(&str, bool, bool)]) -> Draft<SearchItem> {
        Draft {
            open: true,
            at_open: at_open.iter().map(|i| item(i)).collect(),
            items: items.iter().map(|i| item(i)).collect(),
            edits: edits.iter().map(|(i, a, c)| (item(i), *a, *c)).collect(),
            batches: 1,
            confirm: None,
        }
    }

    fn change(d: &Draft<SearchItem>) -> Option<(String, TokenizerChangeKind)> {
        save_change(d).map(|c| (c.item.label().to_string(), c.kind))
    }

    // FR26: Save names the last edit that still holds.
    #[test]
    fn fr26_save_names_the_last_edit_that_holds() {
        assert_eq!(
            change(&draft(&["a"], &["a"], &[])),
            None,
            "unchanged: nothing"
        );
        assert_eq!(
            change(&draft(&["a"], &["a", "b"], &[("b", true, false)])),
            Some(("b".into(), TokenizerChangeKind::Add))
        );
        assert_eq!(
            change(&draft(&["a"], &["a", "c"], &[("c", true, true)])),
            Some(("c".into(), TokenizerChangeKind::Create))
        );
        assert_eq!(
            change(&draft(&["a", "b"], &["b"], &[("a", false, false)])),
            Some(("a".into(), TokenizerChangeKind::Remove))
        );
        // The last edit was undone; an earlier removal still holds.
        assert_eq!(
            change(&draft(
                &["a", "b"],
                &["b"],
                &[("a", false, false), ("c", true, false), ("c", false, false)]
            )),
            Some(("a".into(), TokenizerChangeKind::Remove))
        );
        // Toggled back and forth: unchanged.
        assert_eq!(
            change(&draft(
                &["a"],
                &["a"],
                &[("a", false, false), ("a", true, false)]
            )),
            None
        );
    }
}
