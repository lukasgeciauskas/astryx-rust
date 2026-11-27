//! `EmojiPicker`: a standalone emoji panel, a search field over every emoji by CLDR group
//! with the host's frequent emoji first (upstream lab `EmojiPicker`). Picking one calls
//! `on_select`; the picker keeps no selection of its own. Hosts place it inline or inside a
//! Popover.
//!
//! The bundled set ([`EMOJI_GROUPS`]) is generated from upstream's `emojiData.ts`
//! (emojibase-data, built from Unicode CLDR) by `cargo xtask codegen`. Upstream loads it
//! lazily, the first time a picker mounts; here it is compiled in, so the picker never
//! shows its loading or load-error state for it.
//!
//! The emoji sit in rows of eight, one roving tab stop across every group; the arrow
//! keys move by a cell or a row (mirrored right to left), Down from the search field
//! enters the grid and Up from its first row returns to the field. Search matches every
//! word of the query against the start of a word of an emoji's name or keywords.

pub mod examples;
pub(crate) mod grid;

#[path = "generated/emoji_data.rs"]
#[rustfmt::skip]
mod emoji_data;

#[cfg(test)]
mod tests;

use std::collections::HashSet;
use std::rc::Rc;
use std::sync::OnceLock;

use astryx_components::doc::{ComponentDoc, PropMapping, RustApi};
use astryx_components::icon::{Icon, IconColor, IconSize};
use astryx_components::input::TextInput;
use astryx_components::scrollable_area::ScrollableArea;
use astryx_components::text::{Text, TextColor, TextType};
use astryx_i18n::{Arg, current as i18n, use_direction};
use astryx_primitives::scroll_area::ScrollOverscroll;
use astryx_primitives::{is_focus_visible, request_focus};
use astryx_theme::current;
use gpui::prelude::*;
use gpui::{
    App, ElementId, FocusHandle, IntoElement, KeyDownEvent, Length, Role, SharedString,
    StyleRefinement, Window, div, px,
};

pub use emoji_data::{EMOJI_GROUPS, EmojiDatum, EmojiDatumGroup};
use grid::{GridKey, GridMove, first_focusable, grid_move};

/// One emoji the picker offers (upstream `EmojiOption`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EmojiOption {
    /// The emoji character, e.g. "🎉".
    pub emoji: SharedString,
    /// Its name, used for search and as its accessible label, e.g. "Party popper".
    pub name: SharedString,
    /// Other words search should match, e.g. ["celebration", "tada"].
    pub keywords: Vec<SharedString>,
}

impl EmojiOption {
    /// An emoji with its name and no keywords.
    pub fn new(emoji: impl Into<SharedString>, name: impl Into<SharedString>) -> Self {
        EmojiOption {
            emoji: emoji.into(),
            name: name.into(),
            keywords: Vec::new(),
        }
    }

    /// Other words search should match (upstream `keywords`).
    pub fn keywords(mut self, keywords: impl IntoIterator<Item = impl Into<SharedString>>) -> Self {
        self.keywords = keywords.into_iter().map(Into::into).collect();
        self
    }
}

/// A group of emoji under a heading (upstream `EmojiCategory`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EmojiCategory {
    /// The group's heading, e.g. "Smileys & emotion".
    pub name: SharedString,
    /// The group's emoji, in order.
    pub emojis: Vec<EmojiOption>,
    /// The catalog key of a bundled group's heading.
    label_key: Option<&'static str>,
}

impl EmojiCategory {
    /// A group named `name` holding `emojis`.
    pub fn new(
        name: impl Into<SharedString>,
        emojis: impl IntoIterator<Item = EmojiOption>,
    ) -> Self {
        EmojiCategory {
            name: name.into(),
            emojis: emojis.into_iter().collect(),
            label_key: None,
        }
    }
}

/// The emoji listed first, before the groups, when the host passes none (upstream
/// `DEFAULT_FREQUENT_EMOJIS`).
pub fn default_frequent_emojis() -> Vec<EmojiOption> {
    [
        ("👍", "Thumbs up"),
        ("❤️", "Red heart"),
        ("😂", "Face with tears of joy"),
        ("🎉", "Party popper"),
        ("😮", "Face with open mouth"),
        ("🙏", "Folded hands"),
        ("👀", "Eyes"),
        ("✅", "Check mark button"),
    ]
    .into_iter()
    .map(|(e, n)| EmojiOption::new(e, n))
    .collect()
}

/// The catalog keys of the bundled groups' headings (upstream `BUNDLED_GROUP_KEYS`).
const BUNDLED_GROUP_KEYS: [(&str, &str); 9] = [
    (
        "Smileys & emotion",
        "@astryx.emojiPicker.groupSmileysEmotion",
    ),
    ("People & body", "@astryx.emojiPicker.groupPeopleBody"),
    ("Animals & nature", "@astryx.emojiPicker.groupAnimalsNature"),
    ("Food & drink", "@astryx.emojiPicker.groupFoodDrink"),
    ("Travel & places", "@astryx.emojiPicker.groupTravelPlaces"),
    ("Activities", "@astryx.emojiPicker.groupActivities"),
    ("Objects", "@astryx.emojiPicker.groupObjects"),
    ("Symbols", "@astryx.emojiPicker.groupSymbols"),
    ("Flags", "@astryx.emojiPicker.groupFlags"),
];

/// The bundled set as categories (upstream `loadBundledCategories`): every emoji through
/// Emoji 15 with English CLDR names; an empty keyword list is none.
pub fn bundled_categories() -> &'static [EmojiCategory] {
    static BUNDLED: OnceLock<Vec<EmojiCategory>> = OnceLock::new();
    BUNDLED.get_or_init(|| {
        EMOJI_GROUPS
            .iter()
            .map(|group| EmojiCategory {
                name: group.name.into(),
                emojis: group
                    .emojis
                    .iter()
                    .map(|&(emoji, name, keywords)| EmojiOption {
                        emoji: emoji.into(),
                        name: name.into(),
                        keywords: if keywords.is_empty() {
                            Vec::new()
                        } else {
                            keywords
                                .split(' ')
                                .map(|k| SharedString::from(k.to_string()))
                                .collect()
                        },
                    })
                    .collect(),
                label_key: BUNDLED_GROUP_KEYS
                    .iter()
                    .find(|(n, _)| *n == group.name)
                    .map(|(_, k)| *k),
            })
            .collect()
    })
}

/// A query's words (upstream: lower-cased and split on whitespace, empties dropped).
pub fn query_words(query: &str) -> Vec<String> {
    query
        .to_lowercase()
        .split(char::is_whitespace)
        .filter(|w| !w.is_empty())
        .map(str::to_string)
        .collect()
}

/// Whether an emoji's name or keywords hold every word of a search, each as the start
/// of a word (upstream `emojiMatches`): `cat` finds Cat face, not Vacation.
pub fn emoji_matches(option: &EmojiOption, query_words: &[String]) -> bool {
    let mut text = option.name.to_string();
    for keyword in &option.keywords {
        text.push(' ');
        text.push_str(keyword);
    }
    let text = text.to_lowercase();
    let words: Vec<&str> = text
        .split(|c: char| c.is_whitespace() || matches!(c, ':' | ',' | '(' | ')' | '-'))
        .collect();
    query_words
        .iter()
        .all(|query| words.iter().any(|word| word.starts_with(query.as_str())))
}

/// A section of the list: a heading over its emoji (upstream `Section`).
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Section {
    pub key: SharedString,
    pub label: SharedString,
    pub emojis: Vec<EmojiOption>,
}

/// The list's sections for a search (upstream's `sections`): with no query, the frequent
/// row (unless empty) then every group; with one, the matches, de-duplicated by emoji
/// after matching, under "Search results", or nothing.
pub(crate) fn sections(
    frequent: &[EmojiOption],
    groups: &[EmojiCategory],
    words: &[String],
    label_of: &dyn Fn(&EmojiCategory) -> SharedString,
    frequent_label: SharedString,
    results_label: SharedString,
) -> Vec<Section> {
    if words.is_empty() {
        let mut out = Vec::new();
        if !frequent.is_empty() {
            out.push(Section {
                key: "frequent".into(),
                label: frequent_label,
                emojis: frequent.to_vec(),
            });
        }
        for (index, group) in groups.iter().enumerate() {
            out.push(Section {
                key: format!("group-{index}").into(),
                label: label_of(group),
                emojis: group.emojis.clone(),
            });
        }
        return out;
    }
    // Match before de-duplicating: a frequent emoji may carry no keywords, so its
    // group's entry is often the one that matches.
    let mut seen = HashSet::new();
    let matches: Vec<EmojiOption> = frequent
        .iter()
        .chain(groups.iter().flat_map(|g| g.emojis.iter()))
        .filter(|option| emoji_matches(option, words))
        .filter(|option| seen.insert(option.emoji.clone()))
        .cloned()
        .collect();
    if matches.is_empty() {
        Vec::new()
    } else {
        vec![Section {
            key: "results".into(),
            label: results_label,
            emojis: matches,
        }]
    }
}

/// Eight emoji a row, so the arrow keys move through a fixed geometry (upstream
/// `COLUMNS`).
pub const COLUMNS: usize = 8;

/// A section's emoji in rows of eight, the last row padded with empty cells so every row
/// has the grid's full geometry (upstream `toRows`).
pub(crate) fn to_rows(emojis: &[EmojiOption]) -> Vec<Vec<Option<EmojiOption>>> {
    emojis
        .chunks(COLUMNS)
        .map(|chunk| {
            let mut row: Vec<Option<EmojiOption>> = chunk.iter().cloned().map(Some).collect();
            row.resize(COLUMNS, None);
            row
        })
        .collect()
}

type SelectFn = Rc<dyn Fn(SharedString, &EmojiOption, &mut Window, &mut App)>;

/// A standalone emoji panel (upstream lab `EmojiPicker`).
///
/// ```ignore
/// EmojiPicker::new("emoji").on_select(|emoji, _option, _, _| { /* insert it */ })
/// ```
#[derive(IntoElement)]
pub struct EmojiPicker {
    id: ElementId,
    on_select: Option<SelectFn>,
    frequent: Option<Vec<EmojiOption>>,
    categories: Option<Vec<EmojiCategory>>,
    height: Option<Length>,
    auto_focus: bool,
    label: Option<SharedString>,
    search_label: Option<SharedString>,
    style: StyleRefinement,
}

impl EmojiPicker {
    /// A picker over the bundled set, with the default frequent emoji first.
    pub fn new(id: impl Into<ElementId>) -> Self {
        EmojiPicker {
            id: id.into(),
            on_select: None,
            frequent: None,
            categories: None,
            height: None,
            auto_focus: false,
            label: None,
            search_label: None,
            style: StyleRefinement::default(),
        }
    }

    /// Upstream `onSelect`: the picked emoji character and its option.
    pub fn on_select(
        mut self,
        f: impl Fn(SharedString, &EmojiOption, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_select = Some(Rc::new(f));
        self
    }

    /// Upstream `frequentEmojis`: emoji listed first, under "Frequently used"; an empty
    /// list leaves the row out. Defaults to [`default_frequent_emojis`].
    pub fn frequent_emojis(mut self, emojis: impl IntoIterator<Item = EmojiOption>) -> Self {
        self.frequent = Some(emojis.into_iter().collect());
        self
    }

    /// Upstream `categories`: the emoji to offer, by group. Without them the picker
    /// offers the bundled set ([`bundled_categories`]).
    pub fn categories(mut self, categories: impl IntoIterator<Item = EmojiCategory>) -> Self {
        self.categories = Some(categories.into_iter().collect());
        self
    }

    /// Upstream `height`: the scrolling list's height (ten times `--spacing-8` by default).
    pub fn height(mut self, height: impl Into<Length>) -> Self {
        self.height = Some(height.into());
        self
    }

    /// Upstream `hasAutoFocus`: focuses the search field when the picker mounts.
    pub fn auto_focus(mut self, auto_focus: bool) -> Self {
        self.auto_focus = auto_focus;
        self
    }

    /// Upstream `label`: the emoji list's accessible name ("Emoji", localized, by default).
    pub fn label(mut self, label: impl Into<SharedString>) -> Self {
        self.label = Some(label.into());
        self
    }

    /// Upstream `searchLabel`: the search field's placeholder and hidden label ("Search
    /// emoji", localized, by default).
    pub fn search_label(mut self, label: impl Into<SharedString>) -> Self {
        self.search_label = Some(label.into());
        self
    }
}

impl Styled for EmojiPicker {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

/// What a picker keeps between frames.
#[derive(Default)]
struct PickerState {
    query: SharedString,
    preview: Option<EmojiOption>,
    /// The cell (by index into every position of the grid) that holds the tab stop.
    tab_stop: Option<usize>,
    handles: Vec<FocusHandle>,
}

impl RenderOnce for EmojiPicker {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = current(window, cx);
        let t = theme.tokens().clone();
        let rem = window.rem_size();
        let rtl = use_direction(cx).is_rtl();
        let i18n = i18n(cx);
        let label = self
            .label
            .clone()
            .unwrap_or_else(|| i18n.t("@astryx.emojiPicker.label"));
        let search_label = self
            .search_label
            .clone()
            .unwrap_or_else(|| i18n.t("@astryx.emojiPicker.searchLabel"));
        let state = window.use_keyed_state(
            ElementId::Name(format!("{:?}-emoji-picker", self.id).into()),
            cx,
            |_, _| PickerState::default(),
        );
        let query = state.read(cx).query.clone();
        let words = query_words(&query);
        let frequent = self
            .frequent
            .clone()
            .unwrap_or_else(default_frequent_emojis);
        let groups: Vec<EmojiCategory> = match &self.categories {
            Some(c) => c.clone(),
            None => bundled_categories().to_vec(),
        };
        let label_of = |group: &EmojiCategory| match group.label_key {
            Some(key) => i18n.t(key),
            None => group.name.clone(),
        };
        let sections = sections(
            &frequent,
            &groups,
            &words,
            &label_of,
            i18n.t("@astryx.emojiPicker.frequentlyUsed"),
            i18n.t("@astryx.emojiPicker.searchResults"),
        );
        let first_match = sections.first().and_then(|s| s.emojis.first()).cloned();
        let no_results = (!words.is_empty() && sections.is_empty()).then(|| {
            i18n.format(
                "@astryx.emojiPicker.noResults",
                &[("query", Arg::from(query.trim()))],
            )
        });

        // Every position of the grid, padding included, in order.
        let cells: Vec<Option<EmojiOption>> = sections
            .iter()
            .flat_map(|s| to_rows(&s.emojis).into_iter().flatten())
            .collect();
        let focusable: Rc<Vec<bool>> = Rc::new(cells.iter().map(Option::is_some).collect());
        let handles = state.update(cx, |s, cx| {
            while s.handles.len() < cells.len() {
                s.handles.push(cx.focus_handle());
            }
            s.handles.truncate(cells.len());
            // The tab stop follows focus (upstream `handleFocus`), and is repaired onto the
            // first focusable cell when its cell goes.
            if let Some(i) = s.handles.iter().position(|h| h.is_focused(window)) {
                s.tab_stop = Some(i);
            }
            if s.tab_stop
                .is_none_or(|i| !focusable.get(i).copied().unwrap_or(false))
            {
                s.tab_stop = first_focusable(&focusable);
            }
            s.handles.clone()
        });
        let tab_stop = state.read(cx).tab_stop;

        let search_id = ElementId::Name(format!("{:?}-search", self.id).into());
        let on_select = self.on_select.clone();
        let pick = Rc::new(
            move |option: &EmojiOption, window: &mut Window, cx: &mut App| {
                if let Some(f) = &on_select {
                    f(option.emoji.clone(), option, window, cx);
                }
            },
        );
        let search = {
            let (change, keys) = (state.clone(), state.clone());
            let (pick, first, words_now) = (pick.clone(), first_match.clone(), !words.is_empty());
            let (handles, focusable) = (handles.clone(), focusable.clone());
            let mut input = TextInput::new(search_id.clone(), search_label.clone(), query.clone())
                .label_hidden(true)
                .placeholder(search_label.clone())
                .start_icon(
                    Icon::new("search")
                        .size(IconSize::Sm)
                        .color(IconColor::Secondary),
                )
                .clear(true)
                .auto_focus(self.auto_focus)
                .on_change(move |value, _, cx| {
                    change.update(cx, |s, cx| {
                        s.query = value;
                        s.preview = None;
                        cx.notify();
                    })
                })
                .on_enter({
                    let first = first.clone();
                    move |window, cx| {
                        if words_now && let Some(first) = &first {
                            pick(first, window, cx);
                        }
                    }
                })
                .on_key_down(move |event: &KeyDownEvent, window, cx| {
                    // Down enters the grid at its first emoji.
                    if event.keystroke.key == "down"
                        && first.is_some()
                        && let Some(i) = first_focusable(&focusable)
                    {
                        cx.stop_propagation();
                        keys.update(cx, |s, _| s.tab_stop = Some(i));
                        window.focus(&handles[i], cx);
                    }
                });
            input.style().refine(&StyleRefinement::default());
            input = input.width(gpui::relative(1.));
            input
        };

        // The sections, each a grid of rows under its heading.
        let mut list = div().flex().flex_col().gap(t.spacing.s3).w_full();
        let mut cell_index = 0usize;
        for section in &sections {
            let mut grid_el = div()
                .id(ElementId::Name(
                    format!("{:?}-{}", self.id, section.key).into(),
                ))
                .flex()
                .flex_col()
                .gap(t.spacing.s0_5)
                .w_full()
                .px(t.spacing.s3)
                .role(Role::Grid)
                .aria_label(section.label.clone());
            for (row_index, row) in to_rows(&section.emojis).into_iter().enumerate() {
                let mut row_el = div()
                    .id(ElementId::Name(
                        format!("{:?}-{}-row-{row_index}", self.id, section.key).into(),
                    ))
                    .grid()
                    .grid_cols(COLUMNS as u16)
                    .gap_x(t.spacing.s0_5)
                    .role(Role::Row);
                for option in row {
                    let index = cell_index;
                    cell_index += 1;
                    let mut cell = div()
                        .id(ElementId::Name(
                            format!("{:?}-gridcell-{index}", self.id).into(),
                        ))
                        .flex()
                        .min_w_0()
                        .role(Role::GridCell);
                    if let Some(option) = option {
                        let focus = handles[index].clone().tab_stop(tab_stop == Some(index));
                        let (hover, select, focus_state) =
                            (state.clone(), pick.clone(), state.clone());
                        let (o1, o2, o3) = (option.clone(), option.clone(), option.clone());
                        let selector = format!("{}-cell-{index}", self.id);
                        let mut button = div()
                            .id(ElementId::Name(
                                format!("{:?}-cell-{index}", self.id).into(),
                            ))
                            .debug_selector(move || selector)
                            .track_focus(&focus)
                            .relative()
                            .flex()
                            .items_center()
                            .justify_center()
                            .w_full()
                            .aspect_ratio(1.)
                            .rounded(t.radius.inner)
                            .cursor_pointer()
                            .text_size(t.text.large.size)
                            .line_height(gpui::relative(1.))
                            .hover(|s| s.bg(t.color.overlay_hover))
                            .role(Role::Button)
                            .aria_label(option.name.clone())
                            .child(option.emoji.clone())
                            .on_hover(move |hovered, _, cx| {
                                if *hovered {
                                    hover.update(cx, |s, cx| {
                                        s.preview = Some(o1.clone());
                                        cx.notify();
                                    });
                                }
                            })
                            .on_click(move |_, window, cx| {
                                focus_state.update(cx, |s, _| {
                                    s.tab_stop = Some(index);
                                    s.preview = Some(o3.clone());
                                });
                                select(&o2, window, cx);
                            });
                        if is_focus_visible(&focus, window) {
                            // Inset: the cells sit edge to edge, so an outset ring would
                            // overlap their neighbors.
                            button = button.child(astryx_components::focus::focus_ring_inset(
                                &t,
                                t.radius.inner.to_pixels(rem),
                                rem,
                            ));
                        }
                        cell = cell.child(button);
                    }
                    row_el = row_el.child(cell);
                }
                grid_el = grid_el.child(row_el);
            }
            list = list.child(
                div()
                    .flex()
                    .flex_col()
                    .gap(t.spacing.s1)
                    .w_full()
                    .child({
                        // A group's heading stays at the top of the list while its emoji
                        // scroll under it, on the popover surface the picker usually
                        // sits on.
                        let selector = format!("{}-heading-{}", self.id, section.key);
                        div()
                            .debug_selector(move || selector)
                            .sticky_top(px(0.))
                            .px(t.spacing.s3)
                            .py(t.spacing.s1)
                            .bg(t.color.background_popover)
                            .child(Text::new(section.label.clone()).text_type(TextType::Label))
                    })
                    .child(grid_el),
            );
        }
        // The search status: the no-results message, in a status region.
        let status_selector = format!("{}-status", self.id);
        list = list.child(
            div()
                .id(ElementId::Name(format!("{:?}-status", self.id).into()))
                .debug_selector(move || status_selector)
                .px(t.spacing.s3)
                .role(Role::Status)
                .children(no_results.clone().map(|m| {
                    Text::new(m)
                        .text_type(TextType::Supporting)
                        .color(TextColor::Secondary)
                })),
        );
        // The grid's keys: one roving tab stop across every group.
        let keys = {
            let (state, handles, focusable) = (state.clone(), handles.clone(), focusable.clone());
            let search_id = search_id.clone();
            move |event: &KeyDownEvent, window: &mut Window, cx: &mut App| {
                let Some(key) = GridKey::of(&event.keystroke) else {
                    return;
                };
                let Some(current) = handles.iter().position(|h| h.is_focused(window)) else {
                    return;
                };
                let Some(step) = grid_move(&focusable, current, COLUMNS, key, rtl) else {
                    return;
                };
                cx.stop_propagation();
                match step {
                    GridMove::Focus(i) => {
                        state.update(cx, |s, _| s.tab_stop = Some(i));
                        window.focus(&handles[i], cx);
                    }
                    // Up from the first row returns to the search field.
                    GridMove::Before { offset, .. } if offset == COLUMNS => {
                        request_focus(&search_id, cx);
                        window.refresh();
                    }
                    _ => {}
                }
            }
        };
        let leave = state.clone();
        let list = div()
            .id(ElementId::Name(format!("{:?}-grid", self.id).into()))
            .on_key_down(keys)
            .on_hover(move |hovered, _, cx| {
                if !*hovered {
                    leave.update(cx, |s, cx| {
                        s.preview = None;
                        cx.notify();
                    });
                }
            })
            .child(list);
        let height = self
            .height
            .unwrap_or_else(|| px(f32::from(t.spacing.s8.to_pixels(rem)) * 10.).into());
        let scroll = ScrollableArea::new(
            ElementId::Name(format!("{:?}-list", self.id).into()),
            label.clone(),
        )
        .height(height)
        .overscroll(ScrollOverscroll::Contain)
        .child(list);

        // The footer names the emoji under the pointer or focus; its own name already
        // reaches assistive technology, so the footer is hidden from it.
        let preview = state.read(cx).preview.clone();
        let preview_selector = format!("{}-preview", self.id);
        let footer = div()
            .debug_selector(move || preview_selector)
            .flex()
            .items_center()
            .gap(t.spacing.s2)
            .px(t.spacing.s3)
            .pb(t.spacing.s2)
            .min_h(t.spacing.s8)
            .inert(true)
            .children(preview.as_ref().map(|p| {
                div()
                    .text_size(t.text.large.size)
                    .line_height(gpui::relative(1.))
                    .child(p.emoji.clone())
            }))
            .child(
                Text::new(preview.as_ref().map_or(label.clone(), |p| p.name.clone()))
                    .text_type(TextType::Supporting)
                    .color(TextColor::Secondary)
                    .max_lines(1),
            );

        #[cfg(test)]
        DRAWN.with(|d| {
            d.borrow_mut().insert(
                self.id.to_string(),
                Drawn {
                    sections: sections
                        .iter()
                        .map(|s| {
                            (
                                s.label.clone(),
                                s.emojis.iter().map(|e| e.emoji.clone()).collect(),
                            )
                        })
                        .collect(),
                    status: no_results.clone(),
                    preview: preview.clone().map(|p| p.name),
                    tab_stop,
                    cells: cells
                        .iter()
                        .map(|c| c.as_ref().map(|o| o.emoji.clone()))
                        .collect(),
                    handles: handles.clone(),
                },
            );
        });

        let mut root = div()
            .flex()
            .flex_col()
            .gap(t.spacing.s2)
            .w_full()
            .pt(t.spacing.s3);
        theme.refine(root.style(), "emoji-picker", &[], None);
        root.style().refine(&self.style);
        root.child(div().px(t.spacing.s3).w_full().child(search))
            .child(scroll)
            .child(footer)
    }
}

#[cfg(test)]
thread_local! {
    /// What each picker drew last, by its id, for tests.
    pub(crate) static DRAWN: std::cell::RefCell<std::collections::HashMap<String, Drawn>> = std::cell::RefCell::default();
}

/// What a picker drew, for tests.
#[cfg(test)]
#[derive(Clone)]
pub(crate) struct Drawn {
    pub sections: Vec<(SharedString, Vec<SharedString>)>,
    pub status: Option<SharedString>,
    pub preview: Option<SharedString>,
    pub tab_stop: Option<usize>,
    pub cells: Vec<Option<SharedString>>,
    pub handles: Vec<FocusHandle>,
}

const fn p(prop: &'static str, rust: RustApi) -> PropMapping {
    PropMapping { prop, rust }
}

/// Upstream lab `EmojiPicker`.
pub const EMOJI_PICKER_DOC: ComponentDoc = ComponentDoc {
    name: "EmojiPicker",
    rust_type: "astryx::lab::emoji_picker::EmojiPicker",
    props: &[
        p("onSelect", RustApi::Method("on_select")),
        p("frequentEmojis", RustApi::Method("frequent_emojis")),
        p("categories", RustApi::Method("categories")),
        p("height", RustApi::Method("height")),
        p("hasAutoFocus", RustApi::Method("auto_focus")),
        p("label", RustApi::Method("label")),
        p("searchLabel", RustApi::Method("search_label")),
    ],
    example: include_str!("examples.rs"),
};
