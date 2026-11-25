//! Upstream `CommandPalette/`: a dialog with a query field over results
//! from a [`SearchSource`], and its parts.
//!
//! - The query runs against the source as it is typed; an empty query
//!   shows the bootstrap items. Answers that arrive late for an older query
//!   are dropped, and a failed answer is an empty one (FR10). While a query
//!   is pending, the last answer stays (FR1) and the field shows a spinner.
//! - Results sort into sections by their group, in order of first
//!   appearance, ungrouped ones last.
//! - Keys stay in the field: Up and Down move the highlight (stopping at
//!   the ends), PageUp and PageDown jump to the first and last, Enter picks
//!   the highlighted result; Home and End keep moving the caret (FR6). A new
//!   answer resets the highlight (FR7). Escape is the dialog's (FR5).
//! - Picking reports the item's id through `onValueChange` and closes the
//!   palette; in picker mode the selected value is tinted.
//! - The result count, or the empty text, is announced after each query.

use std::rc::Rc;

use astryx_i18n::{Arg, current as i18n};
use astryx_primitives::editing::{EditorStyle, FieldSettings, TextEditState, text_editor};
use astryx_primitives::{Politeness, announce, prefers_reduced_motion};
use astryx_theme::{ThemeStyled, current};
use gpui::{
    AnyElement, App, ElementId, Entity, IntoElement, KeyDownEvent, Length, ParentElement, Pixels,
    RenderOnce, Role, SharedString, StyleRefinement, Styled, Task, Window, div, prelude::*, px,
    relative,
};
use smallvec::SmallVec;

use super::dialog::{Dialog, DialogPurpose};
use crate::content::Kbd;
use crate::icon::{Icon, IconColor, IconSize};
use crate::layout::SpacingStep;
use crate::search::{SearchAnswer, SearchContext, SearchItem, SearchSource, Searchable};
use crate::status::{Spinner, SpinnerSize};

type OpenFn = Rc<dyn Fn(bool, &mut Window, &mut App)>;
type ValueFn = Rc<dyn Fn(SharedString, &mut Window, &mut App)>;
type RenderItemFn<T> = Rc<dyn Fn(&T, bool, &mut Window, &mut App) -> AnyElement>;
type BuildFn = Rc<dyn Fn(&mut Window, &mut App) -> AnyElement>;
type HoverFn = Rc<dyn Fn(&mut Window, &mut App)>;

// ----------------------------------------------------------------------------
// Parts

/// The query field's options, upstream `CommandPaletteInput`: pass one to
/// [`CommandPalette::input`].
#[derive(Clone, Default)]
pub struct CommandPaletteInput {
    placeholder: Option<SharedString>,
    label: Option<SharedString>,
    auto_focus: Option<bool>,
    end_content: Option<BuildFn>,
    value: Option<SharedString>,
    on_value_change: Option<ValueFn>,
    style: StyleRefinement,
}

impl CommandPaletteInput {
    pub fn new() -> Self {
        Self::default()
    }

    astryx_primitives::setters! {
        /// Upstream `placeholder` ("Search…").
        ?placeholder: SharedString,
        /// Upstream `label`: the field's name ("Search commands"), never
        /// its placeholder.
        ?label: SharedString,
        /// Upstream `hasAutoFocus`: on, except inline.
        ?auto_focus: bool,
        /// Upstream `value`: a controlled query.
        ?value: SharedString,
    }

    /// Upstream `endContent`: after the spinner.
    pub fn end_content(
        mut self,
        f: impl Fn(&mut Window, &mut App) -> AnyElement + 'static,
    ) -> Self {
        self.end_content = Some(Rc::new(f));
        self
    }

    /// Upstream `onValueChange`: the query as it is typed.
    pub fn on_value_change(
        mut self,
        f: impl Fn(SharedString, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_value_change = Some(Rc::new(f));
        self
    }
}

impl Styled for CommandPaletteInput {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

/// The keyboard hints under the results, upstream `CommandPaletteFooter`:
/// ↑ ↓ Navigate, ↵ Select, Esc Close, or the caller's content.
#[derive(IntoElement, Default)]
pub struct CommandPaletteFooter {
    children: SmallVec<[AnyElement; 2]>,
    style: StyleRefinement,
}

impl CommandPaletteFooter {
    pub fn new() -> Self {
        Self::default()
    }
}

impl ParentElement for CommandPaletteFooter {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(elements);
    }
}

impl Styled for CommandPaletteFooter {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl RenderOnce for CommandPaletteFooter {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = current(window, cx);
        let t = theme.tokens();
        let mut el = div()
            .flex()
            .flex_wrap()
            .items_center()
            .gap(t.spacing.s4)
            .px(t.spacing.s4)
            .py(t.spacing.s2)
            .flex_none()
            .text_size(t.text.supporting.size)
            .line_height(relative(t.text.supporting.leading))
            .text_color(t.color.text_secondary);
        if self.children.is_empty() {
            let t18 = i18n(cx);
            let hint = |keys: &[&'static str], label: SharedString| {
                div()
                    .flex()
                    .items_center()
                    .gap(t.spacing.s1)
                    .children(keys.iter().map(|k| Kbd::new(*k)))
                    .child(label)
            };
            el = el
                .child(hint(
                    &["up", "down"],
                    t18.t("@astryx.commandPalette.footer.navigate"),
                ))
                .child(hint(
                    &["enter"],
                    t18.t("@astryx.commandPalette.footer.select"),
                ))
                .child(hint(
                    &["escape"],
                    t18.t("@astryx.commandPalette.footer.close"),
                ));
        } else {
            el = el.children(self.children);
        }
        theme.refine(el.style(), "command-palette-footer", &[], None);
        el.style().refine(&self.style);
        el
    }
}

/// The message when there are no results, upstream `CommandPaletteEmpty`.
#[derive(IntoElement)]
pub struct CommandPaletteEmpty {
    content: AnyElement,
}

impl CommandPaletteEmpty {
    pub fn new(content: impl IntoElement) -> Self {
        CommandPaletteEmpty {
            content: content.into_any_element(),
        }
    }
}

impl RenderOnce for CommandPaletteEmpty {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = current(window, cx);
        let t = theme.tokens();
        let mut el = div()
            .flex()
            .items_center()
            .justify_center()
            .py(t.spacing.s8)
            .px(t.spacing.s4)
            .text_size(t.text.supporting.size)
            .line_height(relative(t.text.supporting.leading))
            .text_color(t.color.text_secondary)
            .text_center()
            .child(self.content);
        theme.refine(el.style(), "command-palette-empty", &[], None);
        el
    }
}

/// One result, upstream `CommandPaletteItem`: non-interactive content in an
/// option row.
#[derive(IntoElement)]
pub struct CommandPaletteItem {
    value: SharedString,
    content: AnyElement,
    highlighted: bool,
    selected: bool,
    disabled: bool,
    on_select: Option<ValueFn>,
    on_hover: Option<HoverFn>,
    style: StyleRefinement,
}

impl CommandPaletteItem {
    /// Upstream `value` and `children`.
    pub fn new(value: impl Into<SharedString>, content: impl IntoElement) -> Self {
        CommandPaletteItem {
            value: value.into(),
            content: content.into_any_element(),
            highlighted: false,
            selected: false,
            disabled: false,
            on_select: None,
            on_hover: None,
            style: StyleRefinement::default(),
        }
    }

    astryx_primitives::setters! {
        /// Upstream `isHighlighted`: the keyboard's current result.
        !highlighted,
        /// Upstream `isSelected`: the picked value in picker mode.
        !selected,
        /// Upstream `isDisabled`.
        !disabled,
    }

    /// Upstream `onSelect`: a press picked it.
    pub fn on_select(mut self, f: impl Fn(SharedString, &mut Window, &mut App) + 'static) -> Self {
        self.on_select = Some(Rc::new(f));
        self
    }

    pub(crate) fn on_hover(mut self, f: impl Fn(&mut Window, &mut App) + 'static) -> Self {
        self.on_hover = Some(Rc::new(f));
        self
    }
}

impl Styled for CommandPaletteItem {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl RenderOnce for CommandPaletteItem {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = current(window, cx);
        let t = theme.tokens();
        let mut el = div()
            .id(ElementId::Name(
                format!("command-palette-item-{}", self.value).into(),
            ))
            .debug_selector({
                let value = self.value.clone();
                move || format!("command-palette-item-{value}")
            })
            .flex()
            .items_center()
            .gap(t.spacing.s2)
            .w_full()
            .px(t.spacing.s3)
            .py(t.spacing.s2)
            .rounded(t.radius.inner)
            .text_size(t.text.label.size)
            .line_height(relative(t.text.label.leading))
            .text_color(t.color.text_primary)
            .role(Role::ListBoxOption)
            .aria_selected(self.selected)
            .aria_disabled(self.disabled)
            .child(self.content);
        if self.selected {
            el = el.bg(t.color.accent_muted);
        } else if self.highlighted {
            el = el.bg(t.color.overlay_hover);
        }
        if self.disabled {
            el = el.opacity(0.5);
        } else {
            el = el.cursor_pointer();
            if let Some(f) = self.on_hover {
                el = el.on_mouse_move(move |_, window, cx| f(window, cx));
            }
            if let Some(f) = self.on_select {
                let value = self.value.clone();
                el = el.on_click(move |_, window, cx| {
                    cx.stop_propagation();
                    f(value.clone(), window, cx)
                });
            }
        }
        theme.refine(el.style(), "command-palette-item", &[], None);
        el.style().refine(&self.style);
        el
    }
}

/// A section of results under a heading, upstream `CommandPaletteGroup`.
#[derive(IntoElement)]
pub struct CommandPaletteGroup {
    heading: SharedString,
    children: SmallVec<[AnyElement; 4]>,
    style: StyleRefinement,
}

impl CommandPaletteGroup {
    pub fn new(heading: impl Into<SharedString>) -> Self {
        CommandPaletteGroup {
            heading: heading.into(),
            children: SmallVec::new(),
            style: StyleRefinement::default(),
        }
    }
}

impl ParentElement for CommandPaletteGroup {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(elements);
    }
}

impl Styled for CommandPaletteGroup {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl RenderOnce for CommandPaletteGroup {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = current(window, cx);
        let t = theme.tokens();
        let mut heading = div()
            .px(t.spacing.s3)
            .py(t.spacing.s1)
            .text_size(t.text.supporting.size)
            .line_height(relative(t.text.supporting.leading))
            .text_color(t.color.text_secondary)
            .child(self.heading.clone());
        theme.refine(heading.style(), "command-palette-group-heading", &[], None);
        let mut el = div()
            .id(ElementId::Name(
                format!("command-palette-group-{}", self.heading).into(),
            ))
            .flex()
            .flex_col()
            .gap(t.spacing.s0_5)
            .py(t.spacing.s1)
            .role(Role::Group)
            .aria_label(self.heading)
            .child(heading)
            .children(self.children);
        theme.refine(el.style(), "command-palette-group", &[], None);
        el.style().refine(&self.style);
        el
    }
}

/// The scrolling results, upstream `CommandPaletteList`: a listbox.
#[derive(IntoElement)]
pub struct CommandPaletteList {
    id: ElementId,
    label: Option<SharedString>,
    children: SmallVec<[AnyElement; 8]>,
    style: StyleRefinement,
}

impl CommandPaletteList {
    pub fn new(id: impl Into<ElementId>) -> Self {
        CommandPaletteList {
            id: id.into(),
            label: None,
            children: SmallVec::new(),
            style: StyleRefinement::default(),
        }
    }

    astryx_primitives::setters! {
        /// Upstream `label` ("Commands").
        ?label: SharedString,
    }
}

impl ParentElement for CommandPaletteList {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(elements);
    }
}

impl Styled for CommandPaletteList {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl RenderOnce for CommandPaletteList {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = current(window, cx);
        let t = theme.tokens();
        let label = self
            .label
            .unwrap_or_else(|| i18n(cx).t("@astryx.commandPalette.list.label"));
        let mut el = div()
            .id(self.id)
            .flex()
            .flex_col()
            .gap(t.spacing.s0_5)
            .flex_1()
            .min_h_0()
            .p(t.spacing.s1)
            .overflow_y_scroll()
            .role(Role::ListBox)
            .aria_label(label)
            .children(self.children);
        theme.refine(el.style(), "command-palette-list", &[], None);
        el.style().refine(&self.style);
        el
    }
}

// ----------------------------------------------------------------------------
// The palette

/// A palette's state between frames.
struct PaletteState<T> {
    query: String,
    /// The rows shown.
    results: Vec<T>,
    /// An answer has been shown since the palette opened.
    committed: bool,
    pending: bool,
    /// A partial list was shown for the current query.
    partial: bool,
    version: u64,
    task: Option<Task<()>>,
    highlight: Option<usize>,
    /// The picked value, uncontrolled.
    value: SharedString,
    /// The source the results came from.
    source: Option<u64>,
    was_open: bool,
}

/// `incoming` after the rows already shown: a later list for the same
/// query keeps them in order and adds new ones after.
fn merged<T: Searchable>(shown: &[T], incoming: Vec<T>) -> Vec<T> {
    if incoming.is_empty() {
        return shown.to_vec();
    }
    let mut out: Vec<T> = shown.to_vec();
    for item in incoming {
        if !out.iter().any(|s| s.id() == item.id()) {
            out.push(item);
        }
    }
    out
}

/// The results in their shown order: groups in order of first appearance,
/// then the ungrouped ones.
fn ordered<T: Searchable>(items: &[T]) -> Vec<(Option<SharedString>, Vec<T>)> {
    let mut groups: Vec<(Option<SharedString>, Vec<T>)> = Vec::new();
    let mut ungrouped = Vec::new();
    for item in items {
        match item.group() {
            Some(g) => match groups.iter_mut().find(|(h, _)| h.as_ref() == Some(&g)) {
                Some((_, list)) => list.push(item.clone()),
                None => groups.push((Some(g), vec![item.clone()])),
            },
            None => ungrouped.push(item.clone()),
        }
    }
    if !ungrouped.is_empty() {
        groups.push((None, ungrouped));
    }
    groups
}

/// A dialog that searches a source and picks a result, upstream
/// `CommandPalette`.
///
/// ```ignore
/// CommandPalette::new("commands", open, static_source(items))
///     .on_open_change(|open, _, _| { /* store it */ })
///     .on_value_change(|id, _, _| run_command(&id))
/// ```
#[derive(IntoElement)]
pub struct CommandPalette<T: Searchable = SearchItem> {
    id: ElementId,
    open: bool,
    on_open_change: Option<OpenFn>,
    source: Rc<dyn SearchSource<T>>,
    /// The source's identity: a change while open re-runs the query (FR9).
    source_identity: Option<u64>,
    input: CommandPaletteInput,
    footer: Option<Option<CommandPaletteFooter>>,
    render_item: Option<RenderItemFn<T>>,
    empty_search: Option<BuildFn>,
    empty_bootstrap: Option<BuildFn>,
    empty_search_text: Option<SharedString>,
    empty_bootstrap_text: Option<SharedString>,
    value: Option<SharedString>,
    on_value_change: Option<ValueFn>,
    label: Option<SharedString>,
    width: Length,
    max_height: Pixels,
    inline: bool,
    style: StyleRefinement,
}

impl<T: Searchable> CommandPalette<T> {
    /// Upstream `isOpen` and `searchSource`. A source built each render has
    /// no identity: give it a [`source_key`](Self::source_key) that changes
    /// when the source does, so an open palette re-runs its query (FR9).
    pub fn new(id: impl Into<ElementId>, open: bool, source: impl SearchSource<T>) -> Self {
        let mut palette = Self::from_rc(id, open, Rc::new(source));
        palette.source_identity = None;
        palette
    }

    /// A source kept between renders: a different `Rc` is a new source.
    pub fn from_rc(id: impl Into<ElementId>, open: bool, source: Rc<dyn SearchSource<T>>) -> Self {
        let identity = Rc::as_ptr(&source) as *const () as usize as u64;
        CommandPalette {
            id: id.into(),
            open,
            on_open_change: None,
            source,
            source_identity: Some(identity),
            input: CommandPaletteInput::default(),
            footer: None,
            render_item: None,
            empty_search: None,
            empty_bootstrap: None,
            empty_search_text: None,
            empty_bootstrap_text: None,
            value: None,
            on_value_change: None,
            label: None,
            width: px(640.).into(),
            max_height: px(480.),
            inline: false,
            style: StyleRefinement::default(),
        }
    }

    /// Upstream `onOpenChange`.
    pub fn on_open_change(mut self, f: impl Fn(bool, &mut Window, &mut App) + 'static) -> Self {
        self.on_open_change = Some(Rc::new(f));
        self
    }

    /// Names the source: when the key changes while the palette is open,
    /// its query runs again against the new source (FR9).
    pub fn source_key(mut self, key: impl std::hash::Hash) -> Self {
        use std::hash::Hasher;
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        key.hash(&mut hasher);
        self.source_identity = Some(hasher.finish());
        self
    }

    /// Upstream `input`: the query field's options.
    pub fn input(mut self, input: CommandPaletteInput) -> Self {
        self.input = input;
        self
    }

    /// Upstream `footer`: the caller's footer, or `None` for none.
    pub fn footer(mut self, footer: Option<CommandPaletteFooter>) -> Self {
        self.footer = Some(footer);
        self
    }

    /// Upstream `renderItem`: a row's non-interactive content, told whether
    /// it is highlighted.
    pub fn render_item(
        mut self,
        f: impl Fn(&T, bool, &mut Window, &mut App) -> AnyElement + 'static,
    ) -> Self {
        self.render_item = Some(Rc::new(f));
        self
    }

    /// Upstream `emptySearchText` ("No results").
    pub fn empty_search_text(mut self, text: impl Into<SharedString>) -> Self {
        self.empty_search_text = Some(text.into());
        self
    }

    /// Upstream `emptySearchText` as rich content; `text` is announced.
    pub fn empty_search(
        mut self,
        text: impl Into<SharedString>,
        f: impl Fn(&mut Window, &mut App) -> AnyElement + 'static,
    ) -> Self {
        self.empty_search_text = Some(text.into());
        self.empty_search = Some(Rc::new(f));
        self
    }

    /// Upstream `emptyBootstrapText` ("Type to search").
    pub fn empty_bootstrap_text(mut self, text: impl Into<SharedString>) -> Self {
        self.empty_bootstrap_text = Some(text.into());
        self
    }

    /// Upstream `emptyBootstrapText` as rich content.
    pub fn empty_bootstrap(
        mut self,
        f: impl Fn(&mut Window, &mut App) -> AnyElement + 'static,
    ) -> Self {
        self.empty_bootstrap = Some(Rc::new(f));
        self
    }

    /// Upstream `value`: the picked value in picker mode.
    pub fn value(mut self, value: impl Into<SharedString>) -> Self {
        self.value = Some(value.into());
        self
    }

    /// Upstream `onValueChange`: a result was picked (its id).
    pub fn on_value_change(
        mut self,
        f: impl Fn(SharedString, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_value_change = Some(Rc::new(f));
        self
    }

    /// Upstream `label` ("Command palette").
    pub fn label(mut self, label: impl Into<SharedString>) -> Self {
        self.label = Some(label.into());
        self
    }

    /// Upstream `width` (640).
    pub fn width(mut self, width: impl Into<Length>) -> Self {
        self.width = width.into();
        self
    }

    /// Upstream `maxHeight` (480).
    pub fn max_height(mut self, height: impl Into<Pixels>) -> Self {
        self.max_height = height.into();
        self
    }

    /// Upstream `isInline`: in the page, not a modal; no auto-focus.
    pub fn inline(mut self, inline: bool) -> Self {
        self.inline = inline;
        self
    }
}

impl<T: Searchable> Styled for CommandPalette<T> {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

/// Runs `query` against the source: drops older answers, shows partial
/// lists, and commits the answer.
fn run_search<T: Searchable>(
    state: &Entity<PaletteState<T>>,
    source: &Rc<dyn SearchSource<T>>,
    query: String,
    empty_text: SharedString,
    window: &mut Window,
    cx: &mut App,
) {
    source.cancel();
    let bootstrap = query.trim().is_empty();
    let version = state.update(cx, |s, _| {
        s.version += 1;
        s.highlight = None;
        s.pending = true;
        s.partial = false;
        s.query = query.clone();
        // Upstream's optimistic rows: the shown ones that still match.
        if !bootstrap && !s.results.is_empty() {
            let lower = query.trim().to_lowercase();
            s.results
                .retain(|item| item.label().to_lowercase().contains(&lower));
        }
        s.version
    });
    let weak = state.downgrade();
    let context = SearchContext::new(move |items: Vec<T>, cx: &mut App| {
        let Some(state) = weak.upgrade() else { return };
        state.update(cx, |s, cx| {
            if s.version != version || !s.pending || items.is_empty() {
                return;
            }
            s.results = if s.partial {
                merged(&s.results, items)
            } else {
                items
            };
            s.partial = true;
            cx.notify();
        });
    });
    let answer = if bootstrap {
        source.bootstrap(cx)
    } else {
        source.search(&query, &context, cx)
    };
    match answer {
        SearchAnswer::Ready(items) => {
            commit(state, version, items, bootstrap, empty_text, window, cx)
        }
        SearchAnswer::Pending(task) => {
            let weak = state.downgrade();
            let wait = window.spawn(cx, async move |cx| {
                let items = task.await.unwrap_or_default();
                cx.update(|window, cx| {
                    if let Some(state) = weak.upgrade() {
                        commit(&state, version, items, bootstrap, empty_text, window, cx);
                    }
                })
                .ok();
            });
            state.update(cx, |s, _| s.task = Some(wait));
        }
    }
    window.refresh();
}

/// Shows an answer, if it is still the current query's, and announces it.
fn commit<T: Searchable>(
    state: &Entity<PaletteState<T>>,
    version: u64,
    items: Vec<T>,
    bootstrap: bool,
    empty_text: SharedString,
    window: &mut Window,
    cx: &mut App,
) {
    let count = state.update(cx, |s, _| {
        if s.version != version {
            return None;
        }
        s.results = if s.partial {
            merged(&s.results, items)
        } else {
            items
        };
        s.committed = true;
        s.pending = false;
        s.partial = false;
        s.highlight = None;
        s.task = None;
        Some(s.results.len())
    });
    let Some(count) = count else { return };
    if !bootstrap {
        let text = if count > 0 {
            i18n(cx).format(
                "@astryx.commandPalette.resultCount",
                &[("count", Arg::Num(count as f64))],
            )
        } else {
            empty_text
        };
        announce(text, Politeness::Polite, window, cx);
    }
    window.refresh();
}

impl<T: Searchable> RenderOnce for CommandPalette<T> {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = current(window, cx);
        let t = theme.tokens();
        let rem = window.rem_size();
        let t18 = i18n(cx);
        let open = self.open || self.inline;
        let source_key = self.source_identity;
        let state = window.use_keyed_state(
            ElementId::Name(format!("{}-palette", self.id).into()),
            cx,
            |_, _| PaletteState::<T> {
                query: String::new(),
                results: Vec::new(),
                committed: false,
                pending: false,
                partial: false,
                version: 0,
                task: None,
                highlight: None,
                value: SharedString::default(),
                source: None,
                was_open: false,
                // filled on open
            },
        );
        let editor = window.use_keyed_state(
            ElementId::Name(format!("{}-palette-query", self.id).into()),
            cx,
            |window, cx| TextEditState::reporting(String::new(), false, window, cx),
        );
        let empty_search_text = self
            .empty_search_text
            .clone()
            .unwrap_or_else(|| t18.t("@astryx.commandPalette.emptySearch"));

        // Opening starts from an empty query and the bootstrap; a new source
        // while open re-runs the query against it (FR9).
        let (was_open, last_source) = {
            let s = state.read(cx);
            (s.was_open, s.source)
        };
        if open && !was_open {
            let query = self
                .input
                .value
                .clone()
                .map(String::from)
                .unwrap_or_default();
            state.update(cx, |s, _| {
                s.was_open = true;
                s.source = source_key;
                s.results.clear();
                s.committed = false;
                s.value = SharedString::default();
            });
            editor.update(cx, |e, cx| e.set_text(&query, cx));
            run_search(
                &state,
                &self.source,
                query,
                empty_search_text.clone(),
                window,
                cx,
            );
        } else if open && source_key.is_some() && last_source != source_key {
            state.update(cx, |s, _| s.source = source_key);
            let query = state.read(cx).query.clone();
            run_search(
                &state,
                &self.source,
                query,
                empty_search_text.clone(),
                window,
                cx,
            );
        } else if !open && was_open {
            self.source.cancel();
            state.update(cx, |s, _| {
                s.was_open = false;
                s.version += 1;
                s.task = None;
                s.query.clear();
                s.results.clear();
                s.pending = false;
            });
        }
        // A controlled query.
        if let Some(value) = &self.input.value
            && open
            && state.read(cx).query != value.as_ref()
        {
            editor.update(cx, |e, cx| e.set_text(value, cx));
            run_search(
                &state,
                &self.source,
                value.to_string(),
                empty_search_text.clone(),
                window,
                cx,
            );
        }

        let picked = self
            .value
            .clone()
            .unwrap_or_else(|| state.read(cx).value.clone());
        let order: Vec<(Option<SharedString>, Vec<T>)> = ordered(&state.read(cx).results);
        let flat: Rc<Vec<T>> = Rc::new(order.iter().flat_map(|(_, items)| items.clone()).collect());
        // A preselected value is highlighted once the bootstrap arrives.
        if state.read(cx).highlight.is_none()
            && state.read(cx).query.is_empty()
            && !picked.is_empty()
            && let Some(at) = flat.iter().position(|i| i.id() == picked)
        {
            state.update(cx, |s, _| s.highlight = Some(at));
        }
        let highlight = state.read(cx).highlight.filter(|&h| h < flat.len());

        // What picking does: report, remember, close.
        let pick: ValueFn = {
            let (state, report, close) = (
                state.clone(),
                self.on_value_change.clone(),
                self.on_open_change.clone(),
            );
            let controlled = self.value.is_some();
            let inline = self.inline;
            Rc::new(move |value, window, cx| {
                if !controlled {
                    state.update(cx, |s, _| s.value = value.clone());
                }
                if let Some(f) = &report {
                    f(value, window, cx);
                }
                if !inline && let Some(f) = &close {
                    f(false, window, cx);
                }
                window.refresh();
            })
        };

        // The query field: typing searches; the result keys stay in it.
        let placeholder = self
            .input
            .placeholder
            .clone()
            .unwrap_or_else(|| t18.t("@astryx.commandPalette.input.placeholder"));
        let field_label = self
            .input
            .label
            .clone()
            .unwrap_or_else(|| t18.t("@astryx.commandPalette.input.label"));
        {
            let (state, source, empty) = (
                state.clone(),
                self.source.clone(),
                empty_search_text.clone(),
            );
            let typed = self.input.on_value_change.clone();
            let keys_state = state.clone();
            let keys_flat = flat.clone();
            let keys_pick = pick.clone();
            editor.update(cx, |e, _| {
                e.set_settings(FieldSettings {
                    spell_check: false,
                    ..FieldSettings::default()
                });
                e.on_change = Some(Rc::new(
                    move |text: String, window: &mut Window, cx: &mut App| {
                        if let Some(f) = &typed {
                            f(text.clone().into(), window, cx);
                        }
                        run_search(&state, &source, text, empty.clone(), window, cx);
                    },
                ));
                e.on_key_down = Some(Rc::new(
                    move |event: &KeyDownEvent, window: &mut Window, cx: &mut App| {
                        let count = keys_flat.len();
                        let key = event.keystroke.key.as_str();
                        if event.keystroke.modifiers.modified() || count == 0 && key != "enter" {
                            return;
                        }
                        let current = keys_state.read(cx).highlight;
                        let next = match key {
                            "down" => Some(current.map_or(0, |c| (c + 1).min(count - 1))),
                            "up" => Some(current.map_or(0, |c| c.saturating_sub(1))),
                            "pageup" => Some(0),
                            "pagedown" => Some(count - 1),
                            "enter" => {
                                cx.stop_propagation();
                                if let Some(item) = current.and_then(|c| keys_flat.get(c)) {
                                    keys_pick(item.id(), window, cx);
                                }
                                return;
                            }
                            _ => None,
                        };
                        if let Some(next) = next {
                            cx.stop_propagation();
                            keys_state.update(cx, |s, _| s.highlight = Some(next));
                            window.refresh();
                        }
                    },
                ));
            });
        }
        let pending = state.read(cx).pending && state.read(cx).committed;
        let style = EditorStyle {
            text_color: t.color.text_primary,
            placeholder: Some(placeholder),
            placeholder_color: t.color.text_secondary,
            selection_color: astryx_theme::color::composite(
                t.color.background_surface,
                gpui::Hsla {
                    a: 0.3,
                    ..t.color.accent
                },
            ),
            caret_color: t.color.text_primary,
            multiline: false,
            min_lines: 1,
            max_lines: Some(1),
            height: None,
            reduced_motion: prefers_reduced_motion(cx),
            spelling_color: t.color.error,
        };
        let field = text_editor("query", &editor, style, cx)
            .flex_1()
            .min_w_0()
            .font_stack(&t.font.family.body)
            .text_size(t.text.body.size)
            .line_height(relative(t.text.body.leading))
            .role(Role::ComboBox)
            .aria_label(field_label)
            .aria_expanded(!flat.is_empty());
        let mut input = div()
            .flex()
            .items_center()
            .gap(t.spacing.s2)
            .px(t.spacing.s4)
            .py(t.spacing.s3)
            .flex_none()
            .child(
                div().flex().items_center().flex_none().child(
                    Icon::new("search")
                        .size(IconSize::Sm)
                        .color(IconColor::Secondary),
                ),
            )
            .child(field)
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(t.spacing.s1)
                    .flex_none()
                    .when(pending, |el| {
                        el.child(
                            Spinner::new("palette-spinner")
                                .size(SpinnerSize::Sm)
                                .decorative(),
                        )
                    })
                    .children(self.input.end_content.as_ref().map(|f| f(window, cx))),
            );
        theme.refine(input.style(), "command-palette-input", &[], None);
        input.style().refine(&self.input.style);
        // The dialog moves focus into its first control, the query field
        // (upstream `hasAutoFocus`); inline, focus stays where it is.
        let _ = self.input.auto_focus;

        // The results.
        let committed = state.read(cx).committed;
        let query_empty = state.read(cx).query.trim().is_empty();
        let render_item = self.render_item.clone();
        let mut position = 0usize;
        let mut rows: Vec<AnyElement> = Vec::new();
        for (heading, items) in order {
            let mut group_rows: Vec<AnyElement> = Vec::new();
            for item in items {
                let at = position;
                position += 1;
                let lit = highlight == Some(at);
                let content = match &render_item {
                    Some(f) => f(&item, lit, window, cx),
                    None => div().child(item.label()).into_any_element(),
                };
                let hover_state = state.clone();
                let select = pick.clone();
                group_rows.push(
                    CommandPaletteItem::new(item.id(), content)
                        .highlighted(lit)
                        .selected(!picked.is_empty() && item.id() == picked)
                        .on_hover(move |window, cx| {
                            if hover_state.read(cx).highlight != Some(at) {
                                hover_state.update(cx, |s, _| s.highlight = Some(at));
                                window.refresh();
                            }
                        })
                        .on_select(move |value, window, cx| select(value, window, cx))
                        .into_any_element(),
                );
            }
            match heading {
                Some(h) => rows.push(
                    CommandPaletteGroup::new(h)
                        .children(group_rows)
                        .into_any_element(),
                ),
                None => rows.extend(group_rows),
            }
        }
        let empty_bootstrap_text = self
            .empty_bootstrap_text
            .clone()
            .unwrap_or_else(|| t18.t("@astryx.commandPalette.emptyBootstrap"));
        let body: AnyElement = if !rows.is_empty() {
            CommandPaletteList::new(ElementId::Name(format!("{}-list", self.id).into()))
                .children(rows)
                .into_any_element()
        } else if !committed {
            // Before the first answer: the empty box's size, no message.
            CommandPaletteEmpty::new(div().invisible().child(empty_bootstrap_text))
                .into_any_element()
        } else if query_empty {
            CommandPaletteEmpty::new(match &self.empty_bootstrap {
                Some(f) => f(window, cx),
                None => empty_bootstrap_text.into_any_element(),
            })
            .into_any_element()
        } else {
            CommandPaletteEmpty::new(match &self.empty_search {
                Some(f) => f(window, cx),
                None => empty_search_text.into_any_element(),
            })
            .into_any_element()
        };
        let footer = match self.footer {
            Some(Some(f)) => Some(f.into_any_element()),
            Some(None) => None,
            None => Some(CommandPaletteFooter::new().into_any_element()),
        };
        let divider = || div().h(px(1.)).flex_none().bg(t.color.border);
        let content = div()
            .flex()
            .flex_col()
            .max_h(self.max_height)
            .min_h_0()
            .child(input)
            .child(divider())
            .child(div().flex().flex_col().flex_1().min_h_0().child(body))
            .when(footer.is_some(), |el| el.child(divider()))
            .children(footer);
        let _ = rem;

        let label = self
            .label
            .clone()
            .unwrap_or_else(|| t18.t("@astryx.commandPalette.label"));
        let report = self.on_open_change.clone();
        let mut dialog = Dialog::new(self.id.clone(), open)
            .width(self.width)
            .max_height(Length::from(self.max_height))
            .purpose(DialogPurpose::Info)
            .padding(SpacingStep::S0)
            .inline(self.inline)
            .label(label)
            .on_open_change(move |open, window, cx| {
                if let Some(f) = &report {
                    f(open, window, cx);
                }
            })
            .child(content);
        *dialog.style() = self.style;
        dialog
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn results_sort_into_groups_in_order_of_appearance() {
        let items = vec![
            SearchItem::new("a", "A"),
            SearchItem::new("b", "B").group("Files"),
            SearchItem::new("c", "C").group("Commands"),
            SearchItem::new("d", "D").group("Files"),
        ];
        let order = ordered(&items);
        let shape: Vec<(Option<&str>, Vec<SharedString>)> = order
            .iter()
            .map(|(h, items)| (h.as_deref(), items.iter().map(|i| i.id()).collect()))
            .collect();
        assert_eq!(
            shape,
            [
                (Some("Files"), vec!["b".into(), "d".into()]),
                (Some("Commands"), vec!["c".into()]),
                (None, vec!["a".into()]),
            ]
        );
    }

    #[test]
    fn a_later_list_keeps_the_rows_shown_and_adds_after_them() {
        let shown = vec![SearchItem::new("a", "A"), SearchItem::new("b", "B")];
        let next = vec![SearchItem::new("b", "B"), SearchItem::new("c", "C")];
        let ids: Vec<SharedString> = merged(&shown, next).iter().map(|i| i.id()).collect();
        assert_eq!(ids, ["a", "b", "c"]);
        assert_eq!(
            merged(&shown, Vec::new()).len(),
            2,
            "an empty list keeps the rows"
        );
    }
}
