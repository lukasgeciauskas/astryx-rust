//! BaseTypeahead, upstream `Typeahead/BaseTypeahead.tsx`: a bare search field
//! over a result list from a [`SearchSource`]. Typeahead and Tokenizer draw
//! the field's chrome around it.
//!
//! - Typing searches (see the engine); the list opens with the first result
//!   highlighted. ArrowDown and ArrowUp wrap, Home and End jump, Enter picks
//!   (nothing while the list still answers an earlier query), Escape and Tab
//!   hide it (FR5). Focus stays in the field; the highlighted option is its
//!   active descendant (fork patch R13).
//! - Picking reports the item, clears the query and closes (FR1).
//! - Results group under their group's heading, ungrouped ones first; an
//!   empty search shows and announces its message (FR4).
//! - `menu` wraps the list with information, or shows a dialog that only
//!   the controller opens (FR13-FR16).
//! - A disabled field keeps its tab stop, read-only, and opens and picks
//!   nothing (FR9).

use std::rc::Rc;
use std::time::Duration;

use astryx_i18n::{current as i18n, use_direction};
use astryx_primitives::editing::{EditorStyle, FieldSettings, TextEditState, text_editor};
use astryx_primitives::{
    EscapeBehavior, LayerAlign, LayerOptions, Placement, Politeness, anchored_layer, announce,
    is_focus_visible, prefers_reduced_motion,
};
use astryx_theme::{ThemeStyled, current};
use gpui::{
    AnyElement, App, Bounds, ElementId, Entity, FocusHandle, Focusable, IntoElement, KeyDownEvent,
    ParentElement, Pixels, RenderOnce, Role, ScrollHandle, SharedString, StyleRefinement, Styled,
    Window, div, prelude::*, px, relative,
};

use super::engine::{Engine, EntriesFn, OpenFn, QueryFn, Search};
use super::item::TypeaheadItem;
use crate::icon::{Icon, IconColor, IconSize};
use crate::search::{SearchSource, Searchable};
use crate::status::{Spinner, SpinnerSize};
use astryx_primitives::ElementSize;

type RenderItemFn<T> = Rc<dyn Fn(&T, &mut Window, &mut App) -> AnyElement>;
type ChangeFn<T> = Rc<dyn Fn(Option<T>, &mut Window, &mut App)>;
type KeyFn = Rc<dyn Fn(&KeyDownEvent, &mut Window, &mut App) -> bool>;
type CloseFn = Rc<dyn Fn(&mut Window, &mut App)>;
type ListboxRender = Rc<dyn Fn(ListboxMenuInfo, &mut Window, &mut App) -> AnyElement>;
type DialogRender = Rc<dyn Fn(DialogMenuInfo, &mut Window, &mut App) -> AnyElement>;
type WrapFn = Box<dyn FnOnce(AnyElement, &mut Window, &mut App) -> AnyElement>;
pub(crate) type TrailingFn = Rc<dyn Fn(&KeyDownEvent, &mut Window, &mut App)>;
type PickFn<T> = Rc<dyn Fn(T, &mut Window, &mut App)>;
type ChromeFn = Box<dyn FnOnce(AnyElement, bool, &mut Window, &mut App) -> AnyElement>;

/// What a listbox menu's `render` receives (upstream
/// `BaseTypeaheadListboxMenuInfo`).
pub struct ListboxMenuInfo {
    /// The result list: place it, unchanged.
    pub listbox: AnyElement,
    pub query: SharedString,
    pub busy: bool,
    /// The search found nothing.
    pub empty_search: bool,
    pub close: CloseFn,
}

/// What a dialog menu's `render` receives (upstream
/// `BaseTypeaheadDialogMenuInfo`).
pub struct DialogMenuInfo {
    pub query: SharedString,
    pub close: CloseFn,
}

/// Upstream `menu`: what the popup shows.
#[derive(Clone, Default)]
pub enum TypeaheadMenu {
    /// The result list.
    #[default]
    Listbox,
    /// The result list with information around it (a header, a footer).
    ListboxWith(ListboxRender),
    /// The caller's content in a dialog named by the label; only the
    /// controller opens it.
    Dialog {
        label: SharedString,
        render: DialogRender,
    },
}

impl TypeaheadMenu {
    /// The list with `render` placing information around it.
    pub fn listbox(
        f: impl Fn(ListboxMenuInfo, &mut Window, &mut App) -> AnyElement + 'static,
    ) -> Self {
        TypeaheadMenu::ListboxWith(Rc::new(f))
    }

    /// A dialog named `label`.
    pub fn dialog(
        label: impl Into<SharedString>,
        f: impl Fn(DialogMenuInfo, &mut Window, &mut App) -> AnyElement + 'static,
    ) -> Self {
        TypeaheadMenu::Dialog {
            label: label.into(),
            render: Rc::new(f),
        }
    }

    fn is_dialog(&self) -> bool {
        matches!(self, TypeaheadMenu::Dialog { .. })
    }
}

/// Opens and closes a typeahead's menu from code, upstream
/// `BaseTypeaheadHandle` (`handleRef`). Make it in the render that draws the
/// typeahead and pass it with `controller`.
#[derive(Clone)]
pub struct TypeaheadController {
    requests: Entity<Requests>,
}

#[derive(Default)]
pub(crate) struct Requests {
    open: bool,
    close: bool,
    /// The dialog menu is showing.
    dialog: bool,
    /// What had focus when the dialog was asked to open.
    return_to: Option<FocusHandle>,
    pending_dialog_focus: bool,
    /// A wrapper's controller asked to focus or blur the field.
    pub(crate) focus: bool,
    pub(crate) blur: bool,
}

impl TypeaheadController {
    pub fn new(id: impl Into<ElementId>, window: &mut Window, cx: &mut App) -> Self {
        let id: ElementId = id.into();
        TypeaheadController {
            requests: requests(&id, window, cx),
        }
    }

    /// Upstream `open()`: a dialog opens at once; a listbox shows (or
    /// loads) its results.
    pub fn open(&self, window: &mut Window, cx: &mut App) {
        let focused = window.focused(cx);
        self.requests.update(cx, |r, _| {
            r.open = true;
            r.close = false;
            r.return_to = focused;
        });
        window.refresh();
    }

    /// Focuses the field (a Tokenizer handle's `focus()`).
    pub(crate) fn focus(&self, window: &mut Window, cx: &mut App) {
        self.requests.update(cx, |r, _| r.focus = true);
        window.refresh();
    }

    /// Moves focus out of the field (a Tokenizer handle's `blur()`).
    pub(crate) fn blur(&self, window: &mut Window, cx: &mut App) {
        self.requests.update(cx, |r, _| r.blur = true);
        window.refresh();
    }

    /// Upstream `close()`.
    pub fn close(&self, window: &mut Window, cx: &mut App) {
        self.requests.update(cx, |r, _| {
            r.close = true;
            r.open = false;
        });
        window.refresh();
    }
}

fn requests(id: &ElementId, window: &mut Window, cx: &mut App) -> Entity<Requests> {
    window.use_keyed_state(
        ElementId::Name(format!("{id}-typeahead-requests").into()),
        cx,
        |_, _| Requests::default(),
    )
}

pub(crate) fn engine<T: Searchable>(
    id: &ElementId,
    window: &mut Window,
    cx: &mut App,
) -> Entity<Engine<T>> {
    window.use_keyed_state(
        ElementId::Name(format!("{id}-typeahead").into()),
        cx,
        |_, _| Engine::default(),
    )
}

/// A typeahead's parts, which a wrapper arranges (upstream's busy lane and
/// anchor).
pub(crate) struct TypeaheadParts {
    /// The field.
    pub input: AnyElement,
    pub busy: bool,
    /// Puts the popup's layer around the anchor.
    pub wrap: WrapFn,
}

/// A bare search field over a result list, upstream `BaseTypeahead`.
#[derive(IntoElement)]
pub struct BaseTypeahead<T: Searchable> {
    pub(crate) id: ElementId,
    pub(crate) source: Rc<dyn SearchSource<T>>,
    pub(crate) source_identity: Option<u64>,
    pub(crate) value: Option<T>,
    pub(crate) on_change: Option<ChangeFn<T>>,
    pub(crate) render_item: Option<RenderItemFn<T>>,
    pub(crate) placeholder: Option<SharedString>,
    pub(crate) entries_on_focus: bool,
    pub(crate) max_menu_items: usize,
    pub(crate) menu_width: Option<Pixels>,
    pub(crate) min_query_length: usize,
    pub(crate) empty_search_text: Option<SharedString>,
    pub(crate) disabled: bool,
    pub(crate) focusable_disabled: bool,
    pub(crate) auto_focus: bool,
    pub(crate) debounce: Duration,
    pub(crate) size: ElementSize,
    pub(crate) on_key_down: Option<KeyFn>,
    pub(crate) on_change_query: Option<QueryFn>,
    pub(crate) on_open_change: Option<OpenFn>,
    pub(crate) menu: TypeaheadMenu,
    pub(crate) controller: Option<TypeaheadController>,
    pub(crate) label: Option<SharedString>,
    pub(crate) description: Option<SharedString>,
    pub(crate) required: bool,
    pub(crate) invalid: bool,
    pub(crate) opening_query: Option<SharedString>,
    pub(crate) exclude: Vec<SharedString>,
    pub(crate) query_entries: Option<EntriesFn<T>>,
    /// Runs after the built-in keys (a wrapper's caller `onKeyDown`).
    pub(crate) trailing_key: Option<TrailingFn>,
    chrome: Option<ChromeFn>,
    /// Tokenizer FR14: a pick from the focus list keeps it open on the next
    /// choice.
    pub(crate) keep_open_on_pick: bool,
    /// The label of the rows appended after the source's (a create row).
    pub(crate) entry_label: Option<SharedString>,
    pub(crate) style: StyleRefinement,
}

impl<T: Searchable> BaseTypeahead<T> {
    /// Upstream `searchSource`. A source built each render has no identity:
    /// name it with [`source_key`](Self::source_key) so a change of source
    /// invalidates the old one's work.
    pub fn new(id: impl Into<ElementId>, source: impl SearchSource<T>) -> Self {
        let mut base = Self::from_rc(id, Rc::new(source));
        base.source_identity = None;
        base
    }

    /// A source kept between renders: a different `Rc` is a new source.
    pub fn from_rc(id: impl Into<ElementId>, source: Rc<dyn SearchSource<T>>) -> Self {
        let identity = Rc::as_ptr(&source) as *const () as usize as u64;
        BaseTypeahead {
            id: id.into(),
            source,
            source_identity: Some(identity),
            value: None,
            on_change: None,
            render_item: None,
            placeholder: None,
            entries_on_focus: false,
            max_menu_items: 10,
            menu_width: None,
            min_query_length: 1,
            empty_search_text: None,
            disabled: false,
            focusable_disabled: false,
            auto_focus: false,
            debounce: Duration::from_millis(150),
            size: ElementSize::Md,
            on_key_down: None,
            on_change_query: None,
            on_open_change: None,
            menu: TypeaheadMenu::Listbox,
            controller: None,
            label: None,
            description: None,
            required: false,
            invalid: false,
            opening_query: None,
            exclude: Vec::new(),
            query_entries: None,
            trailing_key: None,
            chrome: None,
            keep_open_on_pick: false,
            entry_label: None,
            style: StyleRefinement::default(),
        }
    }

    /// Upstream `anchorRef` with the caller's own chrome: `f` places the
    /// field (and gets whether a search is pending, for a spinner), and the
    /// list anchors to what it returns.
    pub fn chrome(
        mut self,
        f: impl FnOnce(AnyElement, bool, &mut Window, &mut App) -> AnyElement + 'static,
    ) -> Self {
        self.chrome = Some(Box::new(f));
        self
    }

    /// Upstream `ariaDescribedBy`: the field's description.
    pub fn description(mut self, description: impl Into<SharedString>) -> Self {
        self.description = Some(description.into());
        self
    }

    /// Names the source: a new key is a new source.
    pub fn source_key(mut self, key: impl std::hash::Hash) -> Self {
        use std::hash::Hasher;
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        key.hash(&mut hasher);
        self.source_identity = Some(hasher.finish());
        self
    }

    /// Upstream `value`: the selected item.
    pub fn value(mut self, value: Option<T>) -> Self {
        self.value = value;
        self
    }

    /// Upstream `onChange`.
    pub fn on_change(mut self, f: impl Fn(Option<T>, &mut Window, &mut App) + 'static) -> Self {
        self.on_change = Some(Rc::new(f));
        self
    }

    /// Upstream `renderItem`: a result's content (TypeaheadItem by default).
    pub fn render_item(
        mut self,
        f: impl Fn(&T, &mut Window, &mut App) -> AnyElement + 'static,
    ) -> Self {
        self.render_item = Some(Rc::new(f));
        self
    }

    astryx_primitives::setters! {
        /// Upstream `placeholder` ("Search…").
        ?placeholder: SharedString,
        /// Upstream `hasEntriesOnFocus`: the bootstrap list on focus.
        !entries_on_focus,
        /// Upstream `menuWidth`: before the viewport clamp.
        ?menu_width: Pixels,
        /// Upstream `emptySearchText` ("No results found").
        ?empty_search_text: SharedString,
        /// Upstream `isDisabled`.
        !disabled,
        /// Upstream `isFocusableDisabled`: disabled but a tab stop.
        !focusable_disabled,
        /// Upstream `hasAutoFocus`.
        !auto_focus,
        /// Upstream `size`: the option rows' padding.
        size: ElementSize,
        /// Upstream `menu`.
        menu: TypeaheadMenu,
        /// The field's accessible name (upstream `aria-labelledby`).
        ?label: SharedString,
    }

    /// Upstream `maxMenuItems`: 10 by default.
    pub fn max_menu_items(mut self, n: usize) -> Self {
        self.max_menu_items = n.max(1);
        self
    }

    /// Upstream `minQueryLength`, in graphemes: 1 by default.
    pub fn min_query_length(mut self, n: usize) -> Self {
        self.min_query_length = n;
        self
    }

    /// Upstream `debounceMs`: 150ms by default; zero searches at once.
    pub fn debounce(mut self, debounce: Duration) -> Self {
        self.debounce = debounce;
        self
    }

    /// Upstream `onKeyDown`: runs first; returning `true` (upstream's
    /// `preventDefault()`) skips the built-in handling.
    pub fn on_key_down(
        mut self,
        f: impl Fn(&KeyDownEvent, &mut Window, &mut App) -> bool + 'static,
    ) -> Self {
        self.on_key_down = Some(Rc::new(f));
        self
    }

    /// Upstream `onChangeQuery`.
    pub fn on_change_query(
        mut self,
        f: impl Fn(SharedString, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_change_query = Some(Rc::new(f));
        self
    }

    /// Upstream `onOpenChange`.
    pub fn on_open_change(mut self, f: impl Fn(bool, &mut Window, &mut App) + 'static) -> Self {
        self.on_open_change = Some(Rc::new(f));
        self
    }

    /// Upstream `handleRef`: the controller that opens and closes the menu.
    pub fn controller(mut self, controller: TypeaheadController) -> Self {
        self.controller = Some(controller);
        self
    }

    /// The field mounts holding this query, searched as if typed (FR18).
    #[allow(dead_code)]
    pub(crate) fn opening_query(mut self, query: impl Into<SharedString>) -> Self {
        self.opening_query = Some(query.into());
        self
    }

    fn inert(&self) -> bool {
        self.disabled || self.focusable_disabled
    }
}

impl<T: Searchable> Styled for BaseTypeahead<T> {
    /// Upstream `xstyle` and `inputXStyle`: the field.
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

/// The rows' bounds, for keyboard scrolling.
#[derive(Default)]
struct RowBounds {
    rows: Vec<Option<Bounds<Pixels>>>,
    reveal: bool,
    scroll: ScrollHandle,
}

impl<T: Searchable> BaseTypeahead<T> {
    /// Builds the parts: the field, the busy state, and the popup layer to
    /// put around an anchor.
    pub(crate) fn parts(self, window: &mut Window, cx: &mut App) -> TypeaheadParts {
        let theme = current(window, cx);
        let t = theme.tokens();
        let rem = window.rem_size();
        let rtl = use_direction(cx).is_rtl();
        let t18 = i18n(cx);
        let engine = engine::<T>(&self.id, window, cx);
        let requests = match &self.controller {
            Some(c) => c.requests.clone(),
            None => requests(&self.id, window, cx),
        };
        let rows_state = window.use_keyed_state(
            ElementId::Name(format!("{}-typeahead-rows", self.id).into()),
            cx,
            |_, _| RowBounds::default(),
        );
        let editor = window.use_keyed_state(
            ElementId::Name(format!("{}-typeahead-input", self.id).into()),
            cx,
            |window, cx| TextEditState::reporting("", false, window, cx),
        );
        let input_focus = editor.read(cx).focus_handle(cx);
        let dialog_flag = self.menu.is_dialog();
        engine.update(cx, |e, _| {
            e.on_open_change = self.on_open_change.clone();
            e.dialog_menu = dialog_flag;
        });
        let inert = self.inert();
        let search = Rc::new(Search {
            engine: engine.clone(),
            source: self.source.clone(),
            max_items: self.max_menu_items,
            min_query_length: self.min_query_length,
            debounce: self.debounce,
            entries_on_focus: self.entries_on_focus,
            exclude: self.exclude.clone(),
            query_entries: self.query_entries.clone(),
            entry_label: self.entry_label.clone(),
        });

        // FR3: a new source invalidates the old one's work.
        if self.source_identity.is_some() && engine.read(cx).source != self.source_identity {
            let first = engine.read(cx).source.is_none();
            engine.update(cx, |e, _| {
                e.source = self.source_identity;
                if !first {
                    e.generation += 1;
                    e.loading = false;
                    e.kept_answer = false;
                    e.list_query = None;
                }
            });
        }
        // FR9: disabling drops work in flight and picks nothing.
        if inert && engine.read(cx).loading {
            engine.update(cx, |e, _| {
                e.generation += 1;
                e.loading = false;
            });
        }
        // FR18: the opening query, read once.
        if let Some(query) = self.opening_query.clone() {
            let read = window.use_keyed_state(
                ElementId::Name(format!("{}-typeahead-opening", self.id).into()),
                cx,
                |_, _| false,
            );
            if !*read.read(cx) {
                read.update(cx, |r, _| *r = true);
                editor.update(cx, |e, cx| e.set_text(&query, cx));
                let search = search.clone();
                let text = query.to_string();
                window.defer(cx, move |window, cx| {
                    search.query_changed(text, true, window, cx)
                });
                let focus = input_focus.clone();
                window.defer(cx, move |window, cx| window.focus(&focus, cx));
            }
        }
        // Auto focus.
        if self.auto_focus {
            let done = window.use_keyed_state(
                ElementId::Name(format!("{}-typeahead-autofocused", self.id).into()),
                cx,
                |_, _| false,
            );
            if !*done.read(cx) {
                done.update(cx, |d, _| *d = true);
                let focus = input_focus.clone();
                window.defer(cx, move |window, cx| window.focus(&focus, cx));
            }
        }
        // The controller's requests (FR16).
        let dialog_menu = self.menu.is_dialog();
        let (open_request, close_request) = {
            let r = requests.read(cx);
            (r.open, r.close)
        };
        if open_request {
            requests.update(cx, |r, _| r.open = false);
            if dialog_menu {
                requests.update(cx, |r, _| {
                    r.dialog = true;
                    r.pending_dialog_focus = true;
                });
                search.show_dialog(window, cx);
            } else if !inert {
                search.open(window, cx);
            }
        }
        if close_request {
            requests.update(cx, |r, _| r.close = false);
            search.hide(window, cx);
        }
        let (focus_request, blur_request) = {
            let r = requests.read(cx);
            (r.focus, r.blur)
        };
        if focus_request || blur_request {
            requests.update(cx, |r, _| {
                r.focus = false;
                r.blur = false;
            });
            if focus_request {
                let focus = input_focus.clone();
                window.defer(cx, move |window, cx| window.focus(&focus, cx));
            } else if input_focus.is_focused(window) {
                window.blur(cx);
            }
        }
        // A dialog shows only while opened by the controller.
        let dialog_open = dialog_menu && requests.read(cx).dialog && engine.read(cx).open;
        if dialog_menu && !engine.read(cx).open && requests.read(cx).dialog {
            requests.update(cx, |r, _| r.dialog = false);
        }

        // The field.
        let editable = !inert;
        {
            let (on_change, on_key) = (search.clone(), search.clone());
            let report = self.on_change_query.clone();
            let caller_key = self.on_key_down.clone();
            let rows = rows_state.clone();
            let pick = self.pick_fn(&search, &editor, window, cx);
            let disabled = self.disabled || self.focusable_disabled;
            let dialog = dialog_menu;
            let close_requests = requests.clone();
            let trailing = self.trailing_key.clone();
            editor.update(cx, |e, _| {
                e.set_settings(FieldSettings {
                    disabled: self.disabled,
                    read_only: !editable,
                    spell_check: false,
                    ..FieldSettings::default()
                });
                e.on_change = editable.then(|| {
                    Rc::new(move |text: String, window: &mut Window, cx: &mut App| {
                        if let Some(f) = &report {
                            f(text.clone().into(), window, cx);
                        }
                        on_change.query_changed(text, true, window, cx);
                    }) as astryx_primitives::editing::ChangeFn
                });
                e.on_key_down = Some(Rc::new(
                    move |event: &KeyDownEvent, window: &mut Window, cx: &mut App| {
                        if !disabled
                            && let Some(f) = &caller_key
                            && f(event, window, cx)
                        {
                            cx.stop_propagation();
                            return;
                        }
                        let handled = handle_key(
                            event,
                            &on_key,
                            &rows,
                            &pick,
                            disabled,
                            dialog,
                            &close_requests,
                            window,
                            cx,
                        );
                        if let Some(f) = &trailing {
                            f(event, window, cx);
                        }
                        if handled {
                            cx.stop_propagation();
                        }
                    },
                ));
            });
        }
        let (open, busy) = {
            let e = engine.read(cx);
            (e.open, e.loading)
        };
        let _ = &input_focus;
        let placeholder = self
            .placeholder
            .clone()
            .unwrap_or_else(|| t18.t("@astryx.typeahead.searchPlaceholder"));
        let style = EditorStyle {
            text_color: t.color.text_primary,
            placeholder: Some(placeholder.clone()),
            placeholder_color: t.color.text_secondary,
            selection_color: crate::input::selection_color(t),
            caret_color: t.color.text_primary,
            multiline: false,
            min_lines: 1,
            max_lines: Some(1),
            height: None,
            reduced_motion: prefers_reduced_motion(cx),
            spelling_color: t.color.error,
        };
        let mut input = text_editor("input", &editor, style, cx)
            .flex_1()
            .min_w(px(60.))
            .font_stack(&t.font.family.body)
            .text_size(t.text.body.size)
            .line_height(relative(t.text.body.leading))
            .role(Role::ComboBox)
            .aria_expanded(open)
            .aria_busy(busy)
            .aria_disabled(inert)
            .aria_read_only(!editable)
            .aria_required(self.required)
            .aria_invalid(self.invalid)
            .aria_placeholder(placeholder);
        if dialog_menu {
            input = input.aria_has_popup(gpui::HasPopup::Dialog);
        }
        if let Some(label) = &self.label {
            input = input.aria_label(label.clone());
        }
        if let Some(description) = &self.description {
            input = input.aria_description(description.clone());
        }
        input.style().refine(&self.style);
        // Focus opens (FR3), entries on focus load (FR2); leaving closes.
        {
            let focus_in = window.use_keyed_state(
                ElementId::Name(format!("{}-typeahead-focus", self.id).into()),
                cx,
                |_, _| (None::<gpui::Subscription>, None::<gpui::Subscription>),
            );
            if focus_in.read(cx).0.is_none() {
                let on_in = search.shared_for_focus();
                let on_out = search.shared_for_focus();
                let inert_flag = inert;
                let a = window.on_focus_in(&input_focus, cx, move |window, cx| {
                    if !inert_flag {
                        on_in.open(window, cx);
                    }
                });
                let b = window.on_focus_out(&input_focus, cx, move |_, window, cx| {
                    // FR14: a dialog menu stays open as focus moves into it.
                    if !on_out.engine.read(cx).dialog_menu {
                        on_out.hide(window, cx);
                    }
                });
                focus_in.update(cx, |s, _| *s = (Some(a), Some(b)));
            }
        }
        let input = input.into_any_element();

        // The popup.
        let id = self.id.clone();
        let menu = self.menu.clone();
        let menu_width = self.menu_width;
        let size = self.size;
        let render_item = self.render_item.clone();
        let value_id = self.value.as_ref().map(|v| v.id());
        let empty_text = self
            .empty_search_text
            .clone()
            .unwrap_or_else(|| t18.t("@astryx.typeahead.emptySearchResults"));
        let pick = self.pick_fn(&search, &editor, window, cx);
        let focus_for_rows = input_focus.clone();
        let wrap = Box::new(
            move |anchor: AnyElement, window: &mut Window, cx: &mut App| {
                let theme = current(window, cx);
                let t = theme.tokens();
                let mut layer = anchored_layer(
                    ElementId::Name(format!("{id}-typeahead-layer").into()),
                    anchor,
                )
                .light_dismiss(true)
                .escape(EscapeBehavior::Close)
                .match_anchor_width(true);
                let (open, results, highlight, keyboard, loading, list_query, has_searched, query) = {
                    let e = search.engine.read(cx);
                    (
                        e.open,
                        e.results.clone(),
                        e.highlight,
                        e.keyboard,
                        e.loading,
                        e.list_query.clone(),
                        e.has_searched,
                        e.query.clone(),
                    )
                };
                if !open {
                    return layer.into_any_element();
                }
                let cap = crate::overlay::viewport_cap(t, window);
                let close: CloseFn = {
                    let search = search.clone();
                    Rc::new(move |window, cx| search.hide(window, cx))
                };
                let content = if let TypeaheadMenu::Dialog { label, render } = &menu {
                    let body = render(
                        DialogMenuInfo {
                            query: query.clone().into(),
                            close: close.clone(),
                        },
                        window,
                        cx,
                    );
                    div()
                        .id(ElementId::Name(format!("{id}-typeahead-dialog").into()))
                        .tab_trap()
                        .role(Role::Dialog)
                        .aria_label(label.clone())
                        .p(t.spacing.s3)
                        .child(body)
                        .child(dialog_focus_probe(&requests))
                        .into_any_element()
                } else {
                    let pending = loading
                        || list_query.as_deref() != Some(query.as_str()) && !query.is_empty();
                    let empty_search = results.is_empty() && has_searched && !pending;
                    let listbox = listbox(
                        &id,
                        &results,
                        highlight,
                        keyboard,
                        empty_search,
                        &empty_text,
                        value_id.as_ref(),
                        size,
                        render_item.as_ref(),
                        &search,
                        &pick,
                        &rows_state,
                        &focus_for_rows,
                        cap.height,
                        window,
                        cx,
                    );
                    // FR4: the empty message is announced once.
                    if empty_search
                        && search.engine.read(cx).announced_empty.as_deref() != Some(query.as_str())
                    {
                        search
                            .engine
                            .update(cx, |e, _| e.announced_empty = Some(query.clone()));
                        announce(empty_text.clone(), Politeness::Polite, window, cx);
                    }
                    match &menu {
                        TypeaheadMenu::ListboxWith(render) => render(
                            ListboxMenuInfo {
                                listbox,
                                query: query.clone().into(),
                                busy: loading,
                                empty_search,
                                close: close.clone(),
                            },
                            window,
                            cx,
                        ),
                        _ => listbox,
                    }
                };
                let mut surface = div()
                    .id(ElementId::Name(format!("{id}-typeahead-popup").into()))
                    .occlude()
                    .flex()
                    .flex_col()
                    .min_w_0()
                    .max_w(cap.width)
                    .max_h(cap.height)
                    .bg(t.color.background_popover)
                    .rounded(t.radius.container)
                    .shadow_token(&t.shadow.low)
                    .font_stack(&t.font.family.body)
                    .text_color(t.color.text_primary)
                    .child(content);
                if let Some(width) = menu_width {
                    surface = surface.w(width.min(cap.width));
                }
                theme.refine(surface.style(), "popover", &[], None);
                theme.refine(surface.style(), "typeahead-popup", &[], None);
                let options = LayerOptions {
                    placement: Placement::Below,
                    align: LayerAlign::Start,
                    offset: t.spacing.s1.to_pixels(window.rem_size()),
                    margin: crate::overlay::gutter(t, window),
                    rtl,
                    priority: 1,
                };
                let dismiss = search.clone();
                layer = layer.surface(surface, options, move |window, cx| dismiss.hide(window, cx));
                let _ = dialog_open;
                layer.into_any_element()
            },
        );
        let _ = rem;
        TypeaheadParts { input, busy, wrap }
    }

    /// Upstream `handleSelect` (FR1): report, clear the query, close, and
    /// keep focus in the field.
    fn pick_fn(
        &self,
        search: &Rc<Search<T>>,
        editor: &Entity<TextEditState>,
        window: &mut Window,
        cx: &mut App,
    ) -> PickFn<T> {
        let (search, editor) = (search.clone(), editor.clone());
        let report = self.on_change.clone();
        let inert = self.inert();
        let keep_open = self.keep_open_on_pick;
        let focus = editor.read(cx).focus_handle(cx);
        let _ = window;
        Rc::new(move |item: T, window: &mut Window, cx: &mut App| {
            if inert {
                return;
            }
            // Tokenizer FR14: picking from the focus list keeps it open with
            // the next remaining choice active (the previous one when the
            // pick was last).
            let kept = keep_open && {
                let e = search.engine.read(cx);
                e.query.is_empty() && e.list_query.as_deref() == Some("") && e.results.len() > 1
            };
            if kept {
                search.engine.update(cx, |e, _| {
                    if let Some(at) = e.results.iter().position(|i| i.id() == item.id()) {
                        e.results.remove(at);
                        e.highlight = Some(at.min(e.results.len().saturating_sub(1)));
                    }
                });
                window.focus(&focus, cx);
                if let Some(f) = &report {
                    f(Some(item), window, cx);
                }
                window.refresh();
                return;
            }
            if let Some(f) = &report {
                f(Some(item), window, cx);
            }
            editor.update(cx, |e, cx| e.set_text("", cx));
            search.reset(window, cx);
            window.focus(&focus, cx);
        })
    }
}

impl<T: Searchable> Search<T> {
    fn shared_for_focus(&self) -> Rc<Search<T>> {
        Rc::new(Search {
            engine: self.engine.clone(),
            source: self.source.clone(),
            max_items: self.max_items,
            min_query_length: self.min_query_length,
            debounce: self.debounce,
            entries_on_focus: self.entries_on_focus,
            exclude: self.exclude.clone(),
            query_entries: self.query_entries.clone(),
            entry_label: self.entry_label.clone(),
        })
    }
}

/// Moves focus into a dialog menu once it is drawn (FR14).
fn dialog_focus_probe(requests: &Entity<Requests>) -> AnyElement {
    let requests = requests.clone();
    gpui::canvas(
        |_, _, _| {},
        move |_, _, window, cx| {
            if !requests.read(cx).pending_dialog_focus {
                return;
            }
            let requests = requests.clone();
            window.defer(cx, move |window, cx| {
                requests.update(cx, |r, _| r.pending_dialog_focus = false);
                window.focus_next(cx);
            });
        },
    )
    .absolute()
    .size_0()
    .into_any_element()
}

/// The field's keys (FR5). Returns whether the key was handled.
#[allow(clippy::too_many_arguments)]
fn handle_key<T: Searchable>(
    event: &KeyDownEvent,
    search: &Rc<Search<T>>,
    rows: &Entity<RowBounds>,
    pick: &PickFn<T>,
    disabled: bool,
    dialog: bool,
    requests: &Entity<Requests>,
    window: &mut Window,
    cx: &mut App,
) -> bool {
    let key = event.keystroke.key.as_str();
    let (open, count, highlight, query, list_query, loading, has_debounce) = {
        let e = search.engine.read(cx);
        (
            e.open,
            e.results.len(),
            e.highlight,
            e.query.clone(),
            e.list_query.clone(),
            e.loading,
            e.debounce.is_some(),
        )
    };
    if disabled {
        if open && matches!(key, "escape" | "enter") {
            search.hide(window, cx);
            return true;
        }
        return false;
    }
    if !open {
        if dialog {
            return false;
        }
        if key == "escape" && (loading || has_debounce) {
            search.dismiss_pending(cx);
            return true;
        }
        if key == "down" && (search.entries_on_focus || !query.is_empty()) {
            search.open(window, cx);
            return true;
        }
        return false;
    }
    if dialog {
        if key == "escape" {
            search.hide(window, cx);
            let target = requests.update(cx, |r, _| r.return_to.take());
            if let Some(target) = target {
                target.focus(window, cx);
            }
            return true;
        }
        return false;
    }
    let move_to = |next: Option<usize>, cx: &mut App| {
        search.engine.update(cx, |e, _| {
            e.highlight = next;
            e.keyboard = true;
        });
        rows.update(cx, |r, _| r.reveal = true);
    };
    match key {
        "down" => {
            if count > 0 {
                move_to(
                    Some(highlight.map_or(0, |h| if h + 1 < count { h + 1 } else { 0 })),
                    cx,
                );
            }
        }
        "up" => {
            if count > 0 {
                move_to(
                    Some(highlight.map_or(count - 1, |h| if h > 0 { h - 1 } else { count - 1 })),
                    cx,
                );
            }
        }
        "home" if count > 0 => move_to(Some(0), cx),
        "end" if count > 0 => move_to(Some(count - 1), cx),
        "enter" => {
            // FR5: nothing while the list still answers an earlier query.
            if list_query.as_deref() == Some(query.as_str())
                && let Some(item) =
                    highlight.and_then(|h| search.engine.read(cx).results.get(h).cloned())
            {
                pick(item, window, cx);
            }
        }
        "escape" => search.hide(window, cx),
        _ => return false,
    }
    window.refresh();
    true
}

/// The result list (FR4, FR17).
#[allow(clippy::too_many_arguments)]
fn listbox<T: Searchable>(
    id: &ElementId,
    results: &[T],
    highlight: Option<usize>,
    keyboard: bool,
    empty_search: bool,
    empty_text: &SharedString,
    value_id: Option<&SharedString>,
    size: ElementSize,
    render_item: Option<&RenderItemFn<T>>,
    search: &Rc<Search<T>>,
    pick: &PickFn<T>,
    rows: &Entity<RowBounds>,
    owner: &FocusHandle,
    cap: Pixels,
    window: &mut Window,
    cx: &mut App,
) -> AnyElement {
    let theme = current(window, cx);
    let t = theme.tokens();
    let rem = window.rem_size();
    rows.update(cx, |r, _| r.rows.resize(results.len(), None));
    let scroll = rows.read(cx).scroll.clone();
    let label = i18n(cx).t("@astryx.typeahead.searchResults");
    let source_end = results.len() - search.engine.read(cx).entry_count;
    let entry_label = search.entry_label.clone();
    let mut list = div()
        .id(ElementId::Name(format!("{id}-typeahead-listbox").into()))
        .debug_selector({
            let s = format!("{id}-listbox");
            move || s
        })
        .relative()
        .flex()
        .flex_col()
        .max_h(px(300.).min(cap))
        .overflow_y_scroll()
        .track_scroll(&scroll)
        .p(t.spacing.s1)
        .role(Role::ListBox)
        .aria_label(label);
    theme.refine(list.style(), "typeahead-dropdown", &[], None);
    if empty_search {
        let selector = format!("{id}-empty");
        let mut empty = div()
            .debug_selector(move || selector)
            .p(t.spacing.s3)
            .text_center()
            .text_size(t.text.supporting.size)
            .line_height(relative(t.text.supporting.leading))
            .text_color(t.color.text_secondary)
            .child(empty_text.clone());
        theme.refine(empty.style(), "typeahead-empty-state", &[], None);
        return list.child(empty).into_any_element();
    }
    let (pad_y, pad_x) = match size {
        ElementSize::Sm => (t.spacing.s1, t.spacing.s2),
        ElementSize::Md => (t.spacing.s1_5, t.spacing.s2),
        ElementSize::Lg => (t.spacing.s2, t.spacing.s2),
    };
    let radius = t.radius.element.to_pixels(rem);
    let row = |index: usize, item: &T, window: &mut Window, cx: &mut App| -> AnyElement {
        let lit = highlight == Some(index);
        let selected = value_id == Some(&item.id());
        let content = match (render_item, &entry_label) {
            (_, Some(label)) if index >= source_end => {
                TypeaheadItem::new(label.clone()).into_any_element()
            }
            (Some(f), _) => f(item, window, cx),
            (None, _) => TypeaheadItem::new(item.label()).into_any_element(),
        };
        let selector = format!("{id}-option-{}", item.id());
        let mut el = div()
            .id(ElementId::Name(format!("{id}-option-{index}").into()))
            .debug_selector(move || selector)
            .relative()
            .flex()
            .items_center()
            .w_full()
            .py(pad_y)
            .px(pad_x)
            .rounded(radius)
            .cursor_pointer()
            .role(Role::ListBoxOption)
            .aria_label(item.label())
            .aria_selected(selected)
            .child(
                div()
                    .flex()
                    .flex_1()
                    .min_w_0()
                    .overflow_hidden()
                    .child(content),
            );
        if selected {
            el = el.font_weight(t.font.weight.medium).child(
                Icon::new("check")
                    .size(IconSize::Sm)
                    .color(IconColor::Primary),
            );
        }
        if lit {
            let marker = format!("{id}-highlighted-{}", item.id());
            el = el
                .bg(t.color.overlay_hover)
                .aria_active_descendant_of(owner)
                .child(div().absolute().size_0().debug_selector(move || marker));
            if keyboard && is_focus_visible(owner, window) {
                el = el.child(crate::focus::focus_ring_inset(t, radius, rem));
            }
        }
        {
            let rows = rows.clone();
            el = el.child(
                gpui::canvas(
                    move |bounds, _, cx| {
                        rows.update(cx, |r, _| {
                            if let Some(slot) = r.rows.get_mut(index) {
                                *slot = Some(bounds);
                            }
                        })
                    },
                    |_, _, _, _| {},
                )
                .absolute()
                .top_0()
                .left_0()
                .size_full(),
            );
        }
        {
            let search = search.clone();
            el = el.on_mouse_move(move |_, window, cx| {
                if search.engine.read(cx).highlight != Some(index) {
                    search.engine.update(cx, |e, _| {
                        e.highlight = Some(index);
                        e.keyboard = false;
                    });
                    window.refresh();
                }
            });
        }
        {
            let (pick, item) = (pick.clone(), item.clone());
            el = el.on_click(move |_, window, cx| {
                cx.stop_propagation();
                pick(item.clone(), window, cx)
            });
        }
        let mut props = vec![("size", size.as_str())];
        if lit {
            props.push(("highlighted", "highlighted"));
        }
        if selected {
            props.push(("selected", "selected"));
        }
        theme.refine(el.style(), "typeahead-option", &props, None);
        el.into_any_element()
    };
    // Groups in order of first appearance, ungrouped results first; the
    // caller's appended rows last.
    let mut ungrouped: Vec<usize> = Vec::new();
    let mut groups: Vec<(SharedString, Vec<usize>)> = Vec::new();
    for (i, item) in results[..source_end].iter().enumerate() {
        match item.group() {
            Some(g) => match groups.iter_mut().find(|(h, _)| *h == g) {
                Some((_, list)) => list.push(i),
                None => groups.push((g, vec![i])),
            },
            None => ungrouped.push(i),
        }
    }
    let mut order: Vec<usize> = ungrouped.clone();
    for (_, members) in &groups {
        order.extend(members);
    }
    order.extend(source_end..results.len());
    let mut children: Vec<AnyElement> = ungrouped
        .iter()
        .map(|&i| row(i, &results[i], window, cx))
        .collect();
    for (heading, members) in &groups {
        let mut heading_el = div()
            .px(t.spacing.s2)
            .pt(t.spacing.s2)
            .pb(t.spacing.s1)
            .text_size(t.text.supporting.size)
            .line_height(relative(t.text.supporting.leading))
            .text_color(t.color.text_secondary)
            .child(heading.clone());
        theme.refine(heading_el.style(), "typeahead-group-heading", &[], None);
        let rows_el: Vec<AnyElement> = members
            .iter()
            .map(|&i| row(i, &results[i], window, cx))
            .collect();
        children.push(
            div()
                .id(ElementId::Name(format!("{id}-group-{heading}").into()))
                .flex()
                .flex_col()
                .role(Role::Group)
                .aria_label(heading.clone())
                .child(heading_el)
                .children(rows_el)
                .into_any_element(),
        );
    }
    for (i, item) in results.iter().enumerate().skip(source_end) {
        children.push(row(i, item, window, cx));
    }
    // Keys scroll the highlight into view after the frame.
    let probe = {
        let (rows, highlight) = (rows.clone(), highlight);
        gpui::canvas(
            |_, _, _| {},
            move |_, _, window, cx| {
                if !rows.read(cx).reveal {
                    return;
                }
                let rows = rows.clone();
                window.defer(cx, move |window, cx| {
                    rows.update(cx, |r, _| {
                        r.reveal = false;
                        let Some(row) = highlight.and_then(|h| r.rows.get(h).copied().flatten())
                        else {
                            return;
                        };
                        let viewport = r.scroll.bounds();
                        let mut offset = r.scroll.offset();
                        if row.top() < viewport.top() {
                            offset.y += viewport.top() - row.top();
                        } else if row.bottom() > viewport.bottom() {
                            offset.y -= row.bottom() - viewport.bottom();
                        }
                        r.scroll.set_offset(offset);
                    });
                    window.refresh();
                });
            },
        )
        .absolute()
        .size_0()
    };
    let _ = order;
    list.children(children).child(probe).into_any_element()
}

impl<T: Searchable> RenderOnce for BaseTypeahead<T> {
    fn render(mut self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = current(window, cx);
        let t = theme.tokens();
        let chrome = self.chrome.take();
        let parts = self.parts(window, cx);
        if let Some(chrome) = chrome {
            let anchor = chrome(parts.input, parts.busy, window, cx);
            return (parts.wrap)(anchor, window, cx);
        }
        let anchor = div()
            .flex()
            .items_center()
            .gap(t.spacing.s1)
            .child(parts.input)
            .when(parts.busy, |el| {
                el.child(
                    div().p(t.spacing.s1).child(
                        Spinner::new("typeahead-spinner")
                            .size(SpinnerSize::Sm)
                            .decorative(),
                    ),
                )
            })
            .into_any_element();
        (parts.wrap)(anchor, window, cx)
    }
}
