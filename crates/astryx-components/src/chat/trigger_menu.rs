//! The trigger menu, upstream `Chat/useTriggerMenu.tsx`: a character such as `@` or
//! `/` typed at a word start opens a list of suggestions at the caret, searched from the
//! trigger's `SearchSource` as the query grows; picking a row replaces the trigger and
//! query with text or a token (ChatComposerInput FR10 to FR16).

use std::cell::Cell;
use std::collections::HashMap;
use std::ops::Range;
use std::rc::Rc;
use std::time::Duration;

use astryx_primitives::editing::{OBJECT, TextEditState};
use astryx_primitives::{Politeness, announce};
use astryx_theme::{ThemeStyled as _, current};
use gpui::prelude::*;
use gpui::{
    AnyElement, App, Entity, FocusHandle, Keystroke, MouseButton, Role, SharedString, Task, Window,
    div, px,
};

use super::composer_token::ChatComposerToken;
use crate::search::{SearchAnswer, SearchContext, SearchItem, SearchSource, Searchable};

/// What a trigger inserts for a picked row (upstream `onSelect`'s `string | Token`).
#[derive(Clone)]
pub enum ChatComposerTriggerResult {
    /// Plain text.
    Text(SharedString),
    /// An inline token.
    Token(ChatComposerToken),
}

impl From<ChatComposerToken> for ChatComposerTriggerResult {
    fn from(token: ChatComposerToken) -> Self {
        ChatComposerTriggerResult::Token(token)
    }
}

impl From<&'static str> for ChatComposerTriggerResult {
    fn from(text: &'static str) -> Self {
        ChatComposerTriggerResult::Text(text.into())
    }
}

impl From<String> for ChatComposerTriggerResult {
    fn from(text: String) -> Self {
        ChatComposerTriggerResult::Text(text.into())
    }
}

impl From<SharedString> for ChatComposerTriggerResult {
    fn from(text: SharedString) -> Self {
        ChatComposerTriggerResult::Text(text)
    }
}

type SelectFn = Rc<dyn Fn(&SearchItem) -> ChatComposerTriggerResult>;
type RenderItemFn = Rc<dyn Fn(&SearchItem, &mut Window, &mut App) -> AnyElement>;
type DeserializeFn = Rc<dyn Fn(&str) -> Option<ChatComposerToken>>;
type ElementFn = Rc<dyn Fn(&mut Window, &mut App) -> AnyElement>;

/// Upstream `ChatComposerTrigger`: a trigger character, where its suggestions come
/// from, and what a pick inserts.
///
/// ```ignore
/// ChatComposerTrigger::new('@', static_source(people), |item| {
///     ChatComposerToken::badge(format!("@{}", item.id()), item.label()).into()
/// })
/// ```
#[derive(Clone)]
pub struct ChatComposerTrigger {
    pub(crate) character: char,
    pub(crate) source: Rc<dyn SearchSource<SearchItem>>,
    pub(crate) on_select: SelectFn,
    pub(crate) render_item: Option<RenderItemFn>,
    pub(crate) deserialize: Option<DeserializeFn>,
    pub(crate) empty_search_text: Option<ElementFn>,
    pub(crate) loading_text: Option<SharedString>,
    pub(crate) menu_label: Option<SharedString>,
    pub(crate) multi_word_query: bool,
}

impl ChatComposerTrigger {
    /// Upstream `character`, `searchSource` and `onSelect`.
    pub fn new(
        character: char,
        source: impl SearchSource<SearchItem>,
        on_select: impl Fn(&SearchItem) -> ChatComposerTriggerResult + 'static,
    ) -> Self {
        Self::from_rc(character, Rc::new(source), on_select)
    }

    /// [`Self::new`] with a shared source.
    pub fn from_rc(
        character: char,
        source: Rc<dyn SearchSource<SearchItem>>,
        on_select: impl Fn(&SearchItem) -> ChatComposerTriggerResult + 'static,
    ) -> Self {
        ChatComposerTrigger {
            character,
            source,
            on_select: Rc::new(on_select),
            render_item: None,
            deserialize: None,
            empty_search_text: None,
            loading_text: None,
            menu_label: None,
            multi_word_query: false,
        }
    }

    /// Upstream `renderItem`: a row's content (by default its label).
    pub fn render_item(
        mut self,
        f: impl Fn(&SearchItem, &mut Window, &mut App) -> AnyElement + 'static,
    ) -> Self {
        self.render_item = Some(Rc::new(f));
        self
    }

    /// Upstream `deserialize`: turns a serialized token back into one, when a draft is
    /// restored (a recalled message, a controlled value).
    pub fn deserialize(mut self, f: impl Fn(&str) -> Option<ChatComposerToken> + 'static) -> Self {
        self.deserialize = Some(Rc::new(f));
        self
    }

    /// Upstream `emptySearchText` as text: shown when the query matched nothing (an
    /// empty string shows nothing). Default "No results".
    pub fn empty_search_text(mut self, text: impl Into<SharedString>) -> Self {
        let text: SharedString = text.into();
        self.empty_search_text = Some(Rc::new(move |_, _| text.clone().into_any_element()));
        self
    }

    /// Upstream `emptySearchText` as any content: a link, a create row.
    pub fn empty_search_content(
        mut self,
        f: impl Fn(&mut Window, &mut App) -> AnyElement + 'static,
    ) -> Self {
        self.empty_search_text = Some(Rc::new(f));
        self
    }

    /// Upstream `loadingText`: the row shown while an async search runs. Default
    /// "Searching…".
    pub fn loading_text(mut self, text: impl Into<SharedString>) -> Self {
        self.loading_text = Some(text.into());
        self
    }

    /// Upstream `menuLabel`: the list's accessible name. Default "Suggestions".
    pub fn menu_label(mut self, label: impl Into<SharedString>) -> Self {
        self.menu_label = Some(label.into());
        self
    }

    /// Upstream `hasMultiWordQuery`: the query runs on through spaces, to a line break
    /// or about 64 characters, and a query matching nothing hides the menu unless
    /// `empty_search_text` is set.
    pub fn multi_word_query(mut self, multi: bool) -> Self {
        self.multi_word_query = multi;
        self
    }

    /// Upstream `character`.
    pub fn character(&self) -> char {
        self.character
    }
}

/// Upstream `MAX_MULTI_WORD_QUERY_LENGTH`, in characters.
const MAX_MULTI_WORD_QUERY: usize = 64;

/// The trigger query the caret is in.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ActiveTrigger {
    /// The trigger's index.
    pub index: usize,
    pub query: String,
    /// Where its character is in `text`, in bytes.
    pub start: usize,
}

/// Upstream `findActiveTrigger` (FR10, FR11), over the text from the line's start to
/// the caret. A trigger character activates at the start or after a space, a tab or a
/// line break; a no-break space (as after a token) and a token are no boundary, and a
/// token ends a query, as a token ends upstream's text node.
pub(crate) fn find_active_trigger(
    text: &str,
    triggers: &[ChatComposerTrigger],
) -> Option<ActiveTrigger> {
    let chars: Vec<(usize, char)> = text.char_indices().collect();
    let mut active: Option<(usize, usize)> = None;
    for (i, &(_, ch)) in chars.iter().enumerate() {
        if ch == '\n' || ch == OBJECT {
            active = None;
            continue;
        }
        if let Some((t, _)) = active
            && !triggers[t].multi_word_query
        {
            if ch == ' ' || ch == '\t' {
                active = None;
            }
            continue;
        }
        if let Some((_, start)) = active
            && i - start > MAX_MULTI_WORD_QUERY
        {
            active = None;
        }
        let boundary = i == 0 || matches!(chars[i - 1].1, ' ' | '\t' | '\n');
        if !boundary {
            continue;
        }
        let Some(t) = triggers.iter().position(|tr| tr.character == ch) else {
            continue;
        };
        if let Some((a, _)) = active
            && triggers[a].character != ch
        {
            continue;
        }
        active = Some((t, i));
    }
    let (index, at) = active?;
    let start = chars[at].0;
    let query = text[start + triggers[index].character.len_utf8()..].to_string();
    if triggers[index].multi_word_query && query.chars().count() > MAX_MULTI_WORD_QUERY {
        return None;
    }
    Some(ActiveTrigger {
        index,
        query,
        start,
    })
}

/// Replaces the trigger and its query (a buffer range) with a picked row's result.
pub(crate) type PickFn = Rc<dyn Fn(Range<usize>, ChatComposerTriggerResult, &mut Window, &mut App)>;

/// An Escape on a query: the menu stays hidden while that query keeps growing.
#[derive(Clone)]
struct Dismissed {
    character: char,
    start: usize,
    query: String,
    /// The draft it was in; a draft replaced from outside ends the dismissal.
    draft: u64,
}

/// Upstream `TriggerMenuState`, kept between frames.
#[derive(Default)]
pub(crate) struct MenuState {
    pub active: Option<usize>,
    pub query: String,
    /// The trigger character's offset in the buffer.
    pub start: usize,
    pub items: Vec<SearchItem>,
    pub highlight: Option<usize>,
    pub loading: bool,
    /// Upstream `isHidden`: an unmatched multi-word query closes the menu.
    pub hidden: bool,
    /// The highlight last moved by key, so it shows a focus ring.
    pub keyboard: bool,
    dismissed: Option<Dismissed>,
    request: u64,
    task: Option<Task<()>>,
    /// Whether each trigger's source answers later (upstream's probe).
    asynchronous: HashMap<usize, bool>,
    announced: bool,
    pub on_pick: Option<PickFn>,
}

/// The trigger menu of one input, for one frame.
#[derive(Clone)]
pub(crate) struct TriggerMenu {
    pub state: Entity<MenuState>,
    pub triggers: Rc<[ChatComposerTrigger]>,
    pub debounce: Duration,
    pub editor: Entity<TextEditState>,
    /// The input's draft generation (see [`Dismissed::draft`]).
    pub draft: Rc<Cell<u64>>,
}

impl TriggerMenu {
    /// The text from the caret's line start to a collapsed caret, and the line's start.
    fn text_before(&self, cx: &App) -> Option<(String, usize)> {
        let buffer = self.editor.read(cx).buffer();
        if !buffer.is_collapsed() {
            return None;
        }
        let caret = buffer.cursor();
        let line = buffer.line_start(caret);
        Some((buffer.text()[line..caret].to_string(), line))
    }

    fn find(&self, cx: &App) -> Option<(ActiveTrigger, usize)> {
        let (text, line) = self.text_before(cx)?;
        find_active_trigger(&text, &self.triggers).map(|found| (found, line))
    }

    pub fn is_active(&self, cx: &App) -> bool {
        self.state.read(cx).active.is_some()
    }

    /// Upstream `isMenuVisible`.
    pub fn is_visible(&self, cx: &App) -> bool {
        let s = self.state.read(cx);
        s.active.is_some() && !s.hidden
    }

    /// Upstream `handleInput`: after each edit (and a click in the editor while open),
    /// finds the query at the caret and opens, updates or closes the menu.
    pub fn handle_input(&self, window: &mut Window, cx: &mut App) {
        if self.triggers.is_empty() {
            return;
        }
        let Some((found, line)) = self.find(cx) else {
            let active = self.state.update(cx, |s, _| {
                s.dismissed = None;
                s.active.is_some()
            });
            if active {
                self.reset(window, cx);
            }
            return;
        };
        let start = line + found.start;
        let character = self.triggers[found.index].character;
        let draft = self.draft.get();
        let keep_hidden = self.state.update(cx, |s, _| {
            match &s.dismissed {
                Some(d) if d.character == character && d.start == start && d.draft == draft => {
                    if found.query.starts_with(&d.query) {
                        return true;
                    }
                    s.dismissed = None;
                }
                _ => s.dismissed = None,
            }
            false
        });
        if keep_hidden {
            return;
        }
        let (fresh, changed) = {
            let s = self.state.read(cx);
            let fresh = s.active.is_none()
                || s.active.map(|a| self.triggers[a].character) != Some(character)
                || s.start != start;
            (fresh, s.query != found.query)
        };
        if fresh {
            self.state.update(cx, |s, _| {
                s.active = Some(found.index);
                s.start = start;
                s.query = found.query.clone();
                s.highlight = Some(0);
                s.hidden = false;
            });
            self.search(found.index, found.query, window, cx);
        } else if changed {
            self.state.update(cx, |s, _| {
                s.query = found.query.clone();
                s.active = Some(found.index);
            });
            self.search(found.index, found.query, window, cx);
        }
        window.refresh();
    }

    /// Upstream `searchItems`: async sources wait out the debounce, sync ones answer
    /// at once.
    fn search(&self, index: usize, query: String, window: &mut Window, cx: &mut App) {
        let request = self.state.update(cx, |s, _| {
            s.task = None;
            s.request += 1;
            s.request
        });
        let source = self.triggers[index].source.clone();
        let known = self.state.read(cx).asynchronous.get(&index).copied();
        let asynchronous = match known {
            Some(known) => known,
            None => {
                // Upstream's probe: an empty search tells whether the source is async.
                let ignore = SearchContext::new(|_: Vec<SearchItem>, _: &mut App| {});
                let probe = matches!(source.search("", &ignore, cx), SearchAnswer::Pending(_));
                if probe {
                    source.cancel();
                }
                self.state
                    .update(cx, |s, _| s.asynchronous.insert(index, probe));
                probe
            }
        };
        if asynchronous && !self.debounce.is_zero() {
            let this = self.clone();
            let wait = self.debounce;
            let task = window.spawn(cx, async move |cx| {
                cx.background_executor().timer(wait).await;
                cx.update(|window, cx| this.run_search(index, query, request, window, cx))
                    .ok();
            });
            self.state.update(cx, |s, _| {
                s.loading = true;
                s.task = Some(task);
            });
        } else {
            self.run_search(index, query, request, window, cx);
        }
    }

    fn run_search(
        &self,
        index: usize,
        query: String,
        request: u64,
        window: &mut Window,
        cx: &mut App,
    ) {
        let source = self.triggers[index].source.clone();
        source.cancel();
        let ignore = SearchContext::new(|_: Vec<SearchItem>, _: &mut App| {});
        match source.search(&query, &ignore, cx) {
            SearchAnswer::Ready(items) => {
                if self.is_current(request, index, window, cx) {
                    self.apply(index, items, window, cx);
                }
            }
            SearchAnswer::Pending(task) => {
                let this = self.clone();
                let work = window.spawn(cx, async move |cx| {
                    let items = task.await.unwrap_or_default();
                    cx.update(|window, cx| {
                        if this.is_current(request, index, window, cx) {
                            this.apply(index, items, window, cx);
                        }
                    })
                    .ok();
                });
                self.state.update(cx, |s, _| {
                    s.loading = true;
                    s.task = Some(work);
                });
            }
        }
        window.refresh();
    }

    /// A result applies only while its request is the latest and the caret is still in
    /// the same query; otherwise the query ended without an edit (a send, a clear), and
    /// the menu resets (FR13).
    fn is_current(&self, request: u64, index: usize, window: &mut Window, cx: &mut App) -> bool {
        let (latest, start) = {
            let s = self.state.read(cx);
            (s.request, s.start)
        };
        if request != latest {
            return false;
        }
        let same = self.find(cx).is_some_and(|(found, line)| {
            self.triggers[found.index].character == self.triggers[index].character
                && line + found.start == start
        });
        if !same {
            self.reset(window, cx);
        }
        same
    }

    fn apply(&self, index: usize, items: Vec<SearchItem>, window: &mut Window, cx: &mut App) {
        let trigger = &self.triggers[index];
        // FR14: an unmatched multi-word query hides the menu, unless the trigger has
        // its own empty message.
        let hidden =
            trigger.multi_word_query && trigger.empty_search_text.is_none() && items.is_empty();
        self.state.update(cx, |s, _| {
            s.highlight = (!items.is_empty()).then_some(0);
            s.items = items;
            s.loading = false;
            s.hidden = hidden;
            s.task = None;
        });
        window.refresh();
    }

    /// Upstream `reset`: ends the query, cancels its searches and forgets an Escape.
    pub fn reset(&self, window: &mut Window, cx: &mut App) {
        let active = self.state.update(cx, |s, _| {
            let active = s.active;
            s.task = None;
            s.request += 1;
            s.dismissed = None;
            s.active = None;
            s.query.clear();
            s.items.clear();
            s.highlight = None;
            s.loading = false;
            s.hidden = false;
            active
        });
        if let Some(index) = active {
            self.triggers.get(index).inspect(|t| t.source.cancel());
        }
        window.refresh();
    }

    /// The popover's own close (a press outside): like a reset, but an Escape
    /// remembered stays.
    pub fn hide(&self, window: &mut Window, cx: &mut App) {
        let dismissed = self.state.read(cx).dismissed.clone();
        self.reset(window, cx);
        self.state.update(cx, |s, _| s.dismissed = dismissed);
    }

    /// Upstream `selectItem`: replaces the trigger and query with the row's result.
    pub fn pick(&self, row: usize, window: &mut Window, cx: &mut App) {
        let (index, start, item, on_pick) = {
            let s = self.state.read(cx);
            let (Some(index), Some(item)) = (s.active, s.items.get(row).cloned()) else {
                return;
            };
            (index, s.start, item, s.on_pick.clone())
        };
        let caret = self.editor.read(cx).buffer().cursor();
        let result = (self.triggers[index].on_select)(&item);
        self.reset(window, cx);
        if let Some(f) = on_pick {
            f(start..caret.max(start), result, window, cx);
        }
    }

    /// Upstream `handleKeyDown`: runs before every other key handling; returns
    /// whether the menu took the key (FR7, FR13, FR15, FR16, AR5).
    pub fn handle_key(&self, keystroke: &Keystroke, window: &mut Window, cx: &mut App) -> bool {
        // FR15: keys during an IME composition are the IME's.
        if self.editor.read(cx).buffer().marked().is_some() {
            return false;
        }
        if !self.is_visible(cx) {
            return false;
        }
        let key = keystroke.key.as_str();
        if key == "escape" {
            let dismissed = {
                let s = self.state.read(cx);
                s.active.map(|a| Dismissed {
                    character: self.triggers[a].character,
                    start: s.start,
                    query: s.query.clone(),
                    draft: self.draft.get(),
                })
            };
            self.reset(window, cx);
            self.state.update(cx, |s, _| s.dismissed = dismissed);
            return true;
        }
        let (pickable, count, highlight) = {
            let s = self.state.read(cx);
            (
                !s.loading && !s.items.is_empty(),
                s.items.len(),
                s.highlight,
            )
        };
        let arrow = key == "up" || key == "down";
        if !pickable {
            // A visible menu keeps the arrows even with nothing to pick.
            return arrow;
        }
        match key {
            "down" | "up" => {
                let next = match (key, highlight) {
                    ("down", Some(h)) if h + 1 < count => h + 1,
                    ("down", _) => 0,
                    (_, Some(h)) if h > 0 => h - 1,
                    _ => count - 1,
                };
                self.state.update(cx, |s, _| {
                    s.highlight = Some(next);
                    s.keyboard = true;
                });
                window.refresh();
                true
            }
            "enter" | "tab" => match highlight.filter(|h| *h < count) {
                Some(row) => {
                    self.pick(row, window, cx);
                    true
                }
                None => false,
            },
            _ => false,
        }
    }

    /// The menu's surface while it shows: upstream's `renderMenu`, a listbox above
    /// the caret. `owner` is the editor's focus, which the highlighted row names as its
    /// active descendant.
    pub fn render(
        &self,
        id: &str,
        owner: &FocusHandle,
        window: &mut Window,
        cx: &mut App,
    ) -> Option<AnyElement> {
        if !self.is_visible(cx) {
            self.state.update(cx, |s, _| s.announced = false);
            return None;
        }
        let theme = current(window, cx);
        let t = theme.tokens();
        let rem = window.rem_size();
        let i18n = astryx_i18n::current(cx);
        let (index, loading, items, highlight, keyboard) = {
            let s = self.state.read(cx);
            (
                s.active?,
                s.loading,
                s.items.clone(),
                s.highlight,
                s.keyboard,
            )
        };
        let trigger = self.triggers[index].clone();
        let loading_text = trigger
            .loading_text
            .clone()
            .unwrap_or_else(|| i18n.t("@astryx.chatTriggerMenu.searching"));
        // The loading row is announced once per episode (upstream useBusyAnnouncement).
        let announced = self.state.read(cx).announced;
        if loading && !announced {
            self.state.update(cx, |s, _| s.announced = true);
            announce(loading_text.clone(), Politeness::Polite, window, cx);
        } else if !loading && announced {
            self.state.update(cx, |s, _| s.announced = false);
        }
        let label = trigger
            .menu_label
            .clone()
            .unwrap_or_else(|| i18n.t("@astryx.chatTriggerMenu.suggestions"));
        let list_id = format!("{id}-trigger-menu");
        let mut list = div()
            .id(gpui::ElementId::Name(list_id.clone().into()))
            .debug_selector({
                let s = list_id.clone();
                move || s
            })
            .flex()
            .flex_col()
            .max_h(px(240.))
            .min_w(px(180.))
            .overflow_y_scroll()
            .p(t.spacing.s1)
            .role(Role::ListBox)
            .aria_label(label);
        theme.refine(list.style(), "trigger-menu", &[], None);
        let secondary = |el: gpui::Div| {
            el.p(t.spacing.s3)
                .text_center()
                .text_style(&t.text.supporting)
                .text_color(t.color.text_secondary)
        };
        if loading {
            let s = format!("{list_id}-loading");
            list = list.child(
                secondary(div())
                    .debug_selector(move || s)
                    .child(loading_text),
            );
        } else if items.is_empty() {
            let empty = match &trigger.empty_search_text {
                Some(f) => f(window, cx),
                None => i18n
                    .t("@astryx.chatTriggerMenu.noResults")
                    .into_any_element(),
            };
            let s = format!("{list_id}-empty");
            list = list.child(secondary(div()).debug_selector(move || s).child(empty));
        } else {
            // Upstream `groupItems`: rows grouped by their group, in first-seen order.
            let mut groups: Vec<(Option<SharedString>, Vec<usize>)> = Vec::new();
            for (row, item) in items.iter().enumerate() {
                let group = item.group();
                match groups.iter_mut().find(|(g, _)| *g == group) {
                    Some((_, rows)) => rows.push(row),
                    None => groups.push((group, vec![row])),
                }
            }
            let radius = t.radius.element.to_pixels(rem);
            for (heading, rows) in groups {
                let mut section = div().flex().flex_col();
                if let Some(heading) = heading.clone() {
                    section = section.child(
                        div()
                            .px(t.spacing.s2)
                            .pt(t.spacing.s2)
                            .pb(t.spacing.s1)
                            .text_style(&t.text.supporting)
                            .text_color(t.color.text_secondary)
                            .child(heading),
                    );
                }
                for row in rows {
                    let item = &items[row];
                    let lit = highlight == Some(row);
                    let content = match &trigger.render_item {
                        Some(f) => f(item, window, cx),
                        None => div()
                            .flex_1()
                            .min_w_0()
                            .overflow_hidden()
                            .text_ellipsis()
                            .whitespace_nowrap()
                            .child(item.label())
                            .into_any_element(),
                    };
                    let selector = format!("{list_id}-{}", item.id());
                    let mut el = div()
                        .id(gpui::ElementId::Name(
                            format!("{list_id}-option-{row}").into(),
                        ))
                        .debug_selector(move || selector)
                        .relative()
                        .flex()
                        .items_center()
                        .w_full()
                        .p(t.spacing.s2)
                        .rounded(radius)
                        .cursor_pointer()
                        .font_stack(&t.font.family.body)
                        .text_style(&t.text.body)
                        .text_color(t.color.text_primary)
                        .role(Role::ListBoxOption)
                        .aria_label(item.label())
                        .aria_selected(lit)
                        .child(content);
                    if lit {
                        let marker = format!("{list_id}-highlighted-{}", item.id());
                        el = el
                            .bg(t.color.overlay_hover)
                            .aria_active_descendant_of(owner)
                            .child(div().absolute().size_0().debug_selector(move || marker));
                        if keyboard && astryx_primitives::is_focus_visible(owner, window) {
                            el = el.child(crate::focus::focus_ring_inset(t, radius, rem));
                        }
                    }
                    let (hover, press) = (self.clone(), self.clone());
                    el = el
                        .on_mouse_move(move |_, window, cx| {
                            if hover.state.read(cx).highlight != Some(row) {
                                hover.state.update(cx, |s, _| {
                                    s.highlight = Some(row);
                                    s.keyboard = false;
                                });
                                window.refresh();
                            }
                        })
                        // Upstream picks on press, keeping focus in the editor.
                        .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                            cx.stop_propagation();
                            press.pick(row, window, cx);
                        });
                    section = section.child(el);
                }
                list = list.child(match heading {
                    Some(heading) => section
                        .id(gpui::ElementId::Name(
                            format!("{list_id}-group-{heading}").into(),
                        ))
                        .role(Role::Group)
                        .aria_label(heading)
                        .into_any_element(),
                    None => section.into_any_element(),
                });
            }
        }
        Some(
            div()
                .min_w(px(180.))
                .bg(t.color.background_popover)
                .rounded(t.radius.container)
                .shadow_token(&t.shadow.low)
                .font_stack(&t.font.family.body)
                .child(list)
                .into_any_element(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::search::static_source;

    fn triggers() -> Vec<ChatComposerTrigger> {
        let pick = |_: &SearchItem| ChatComposerTriggerResult::Text("x".into());
        vec![
            ChatComposerTrigger::new('@', static_source(Vec::<SearchItem>::new()), pick),
            ChatComposerTrigger::new('/', static_source(Vec::<SearchItem>::new()), pick),
            ChatComposerTrigger::new('#', static_source(Vec::<SearchItem>::new()), pick)
                .multi_word_query(true),
        ]
    }

    fn find(text: &str) -> Option<(char, String)> {
        let t = triggers();
        find_active_trigger(text, &t).map(|f| (t[f.index].character, f.query))
    }

    #[test]
    fn fr10_a_trigger_starts_the_text_or_follows_white_space() {
        assert_eq!(find("@ad"), Some(('@', "ad".into())));
        assert_eq!(find("hi @ad"), Some(('@', "ad".into())));
        assert_eq!(find("hi\t/cmd"), Some(('/', "cmd".into())));
        assert_eq!(find("a\n@b"), Some(('@', "b".into())));
        assert_eq!(find("mail@ad"), None, "inside a word");
        assert_eq!(find("x\u{A0}@ad"), None, "a no-break space is no boundary");
        assert_eq!(find("\u{FFFC}@ad"), None, "right after a token");
        assert_eq!(find("@ad mi"), None, "a single-token query ends at a space");
        assert_eq!(find("@ad\n"), None, "and at a line break");
    }

    #[test]
    fn fr11_a_multi_word_query_runs_through_spaces_to_its_bound() {
        assert_eq!(
            find("#Design Review: Q4"),
            Some(('#', "Design Review: Q4".into()))
        );
        assert_eq!(
            find("#a @b"),
            Some(('#', "a @b".into())),
            "another trigger is text"
        );
        assert_eq!(
            find("#a #b"),
            Some(('#', "b".into())),
            "the same one starts anew"
        );
        let long = format!("#{}", "a".repeat(65));
        assert_eq!(find(&long), None, "past 64 characters");
        assert_eq!(find("#a\nb"), None, "a line break ends it");
    }
}
