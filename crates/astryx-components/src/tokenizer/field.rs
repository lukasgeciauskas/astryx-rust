//! The Tokenizer field, upstream `Tokenizer/Tokenizer.tsx` (inline presentation).

use std::rc::Rc;
use std::time::Duration;

use astryx_i18n::{current as i18n, use_direction};
use astryx_primitives::{
    ElementSize, LayerAlign, LayerOptions, LayerPosition, Placement, Politeness, anchored_layer,
    announce, is_focus_visible, provide, resolve_size,
};
use astryx_theme::{current, scope};
use gpui::{
    AnyElement, App, ElementId, Focusable, IntoElement, KeyDownEvent, Length, ParentElement,
    Pixels, RenderOnce, Role, SharedString, StyleRefinement, Styled, Window, div, point,
    prelude::*, px,
};

use crate::button::{ButtonVariant, IconButton};
use crate::field::{Field, InputStatus, StatusPlacement, StatusType};
use crate::icon::{Icon, IconColor, IconDefaultSize, IconSize};
use crate::input::control::{Chrome, control_box, described_by, status_indicator};
use crate::input::group::{group_context, group_disabled, group_place, inside_member};
use crate::overflow::OverflowList;
use crate::search::{SearchSource, Searchable};
use crate::status::{Badge, Spinner, SpinnerSize};
use crate::token::Token;
use crate::typeahead::{BaseTypeahead, TypeaheadController, TypeaheadMenu};

type ChangeFn<T> = Rc<dyn Fn(Vec<T>, TokenizerChange<T>, &mut Window, &mut App)>;
type RenderTokenFn<T> = Rc<dyn Fn(&T, ActFn, &mut Window, &mut App) -> AnyElement>;
type CreateFn<T> = Rc<dyn Fn(&str) -> T>;
type ActFn = Rc<dyn Fn(&mut Window, &mut App)>;
type RemoveFn<T> = Rc<dyn Fn(&T, &mut Window, &mut App)>;
type FocusFn = Rc<dyn Fn(&mut Window, &mut App)>;

/// Upstream `TokenizerChange`'s `type`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TokenizerChangeKind {
    Add,
    /// An item made from typed text.
    Create,
    Remove,
    Reorder,
}

/// What changed, upstream `TokenizerChange`.
#[derive(Clone, Debug)]
pub struct TokenizerChange<T> {
    pub item: T,
    pub kind: TokenizerChangeKind,
}

/// Upstream `tokenOverflowBehavior`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TokenOverflowBehavior {
    /// Tokens wrap onto more rows.
    #[default]
    None,
    /// Blurred, the tokens fold into one row with a count.
    UnfocusedInline,
    /// Blurred, the field folds into a one-row placeholder; focus opens it
    /// over the placeholder.
    UnfocusedLayer,
}

/// Focuses, blurs, opens and closes a Tokenizer from code, upstream
/// `TokenizerHandle`. Make it in the render that draws the Tokenizer.
#[derive(Clone)]
pub struct TokenizerController(TypeaheadController);

impl TokenizerController {
    pub fn new(id: impl Into<ElementId>, window: &mut Window, cx: &mut App) -> Self {
        TokenizerController(TypeaheadController::new(id, window, cx))
    }

    /// Upstream `focus()`.
    pub fn focus(&self, window: &mut Window, cx: &mut App) {
        self.0.focus(window, cx)
    }

    /// Upstream `blur()`.
    pub fn blur(&self, window: &mut Window, cx: &mut App) {
        self.0.blur(window, cx)
    }

    /// Upstream `open()`.
    pub fn open(&self, window: &mut Window, cx: &mut App) {
        self.0.open(window, cx)
    }

    /// Upstream `close()`.
    pub fn close(&self, window: &mut Window, cx: &mut App) {
        self.0.close(window, cx)
    }
}

/// Several items from a search, as tokens, upstream `Tokenizer`.
///
/// ```ignore
/// Tokenizer::new("tags", "Tags", static_source(tags), chosen.clone())
///     .on_change(|items, _change, _, _| { /* store them */ })
/// ```
#[derive(IntoElement)]
pub struct Tokenizer<T: Searchable> {
    base: BaseTypeahead<T>,
    label: SharedString,
    value: Vec<T>,
    on_change: Option<ChangeFn<T>>,
    max_entries: Option<usize>,
    clear: bool,
    render_token: Option<RenderTokenFn<T>>,
    disabled: bool,
    disabled_message: Option<SharedString>,
    status: Option<InputStatus>,
    status_placement: StatusPlacement,
    label_hidden: bool,
    description: Option<SharedString>,
    required: bool,
    optional: bool,
    label_tooltip: Option<SharedString>,
    create: Option<CreateFn<T>>,
    start_icon: Option<AnyElement>,
    end_content: Option<AnyElement>,
    width: Option<Length>,
    token_overflow: TokenOverflowBehavior,
    on_focus: Option<FocusFn>,
    on_blur: Option<FocusFn>,
    size: Option<ElementSize>,
    presentation: crate::overlay::PopoverPresentation,
    control_style: StyleRefinement,
    style: StyleRefinement,
}

impl<T: Searchable> Tokenizer<T> {
    /// Upstream `label`, `searchSource` and `value`.
    pub fn new(
        id: impl Into<ElementId>,
        label: impl Into<SharedString>,
        source: impl SearchSource<T>,
        value: Vec<T>,
    ) -> Self {
        Self::from_base(BaseTypeahead::new(id, source), label, value)
    }

    /// A source kept between renders.
    pub fn from_rc(
        id: impl Into<ElementId>,
        label: impl Into<SharedString>,
        source: Rc<dyn SearchSource<T>>,
        value: Vec<T>,
    ) -> Self {
        Self::from_base(BaseTypeahead::from_rc(id, source), label, value)
    }

    fn from_base(base: BaseTypeahead<T>, label: impl Into<SharedString>, value: Vec<T>) -> Self {
        Tokenizer {
            base,
            label: label.into(),
            value,
            on_change: None,
            max_entries: None,
            clear: false,
            render_token: None,
            disabled: false,
            disabled_message: None,
            status: None,
            status_placement: StatusPlacement::Attached,
            label_hidden: false,
            description: None,
            required: false,
            optional: false,
            label_tooltip: None,
            create: None,
            start_icon: None,
            end_content: None,
            width: None,
            token_overflow: TokenOverflowBehavior::None,
            on_focus: None,
            on_blur: None,
            size: None,
            presentation: crate::overlay::PopoverPresentation::Adaptive,
            control_style: StyleRefinement::default(),
            style: StyleRefinement::default(),
        }
    }

    /// Upstream `presentation`: adaptive by default. The bottom-sheet
    /// presentation is not drawn yet; every presentation is the inline field.
    pub fn presentation(mut self, presentation: crate::overlay::PopoverPresentation) -> Self {
        self.presentation = presentation;
        self
    }

    astryx_primitives::setters! {
        /// Upstream `hasClear`: a Clear all button.
        !clear,
        /// Upstream `isDisabled`.
        !disabled,
        /// Upstream `disabledMessage`.
        ?disabled_message: SharedString,
        /// Upstream `status`.
        ?status: InputStatus,
        /// Upstream `statusVariant`.
        status_placement: StatusPlacement,
        /// Upstream `isLabelHidden`.
        !label_hidden,
        /// Upstream `description`.
        ?description: SharedString,
        /// Upstream `isRequired`.
        !required,
        /// Upstream `isOptional`.
        !optional,
        /// Upstream `labelTooltip`.
        ?label_tooltip: SharedString,
        /// Upstream `width`.
        ?width: Length,
        /// Upstream `tokenOverflowBehavior`.
        token_overflow: TokenOverflowBehavior,
        /// Upstream `size`.
        ?size: ElementSize,
    }

    /// Names the source: a new key is a new source.
    pub fn source_key(mut self, key: impl std::hash::Hash) -> Self {
        self.base = self.base.source_key(key);
        self
    }

    /// Upstream `onChange`: the whole selection and what changed.
    pub fn on_change(
        mut self,
        f: impl Fn(Vec<T>, TokenizerChange<T>, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_change = Some(Rc::new(f));
        self
    }

    /// Upstream `maxEntries`: at the limit the field collapses.
    pub fn max_entries(mut self, max: usize) -> Self {
        self.max_entries = Some(max);
        self
    }

    /// Upstream `renderToken`: a token, given its remove action.
    pub fn render_token(
        mut self,
        f: impl Fn(&T, ActFn, &mut Window, &mut App) -> AnyElement + 'static,
    ) -> Self {
        self.render_token = Some(Rc::new(f));
        self
    }

    /// Upstream `renderItem`.
    pub fn render_item(
        mut self,
        f: impl Fn(&T, &mut Window, &mut App) -> AnyElement + 'static,
    ) -> Self {
        self.base = self.base.render_item(f);
        self
    }

    /// Upstream `placeholder`: while nothing is chosen.
    pub fn placeholder(mut self, placeholder: impl Into<SharedString>) -> Self {
        self.base.placeholder = Some(placeholder.into());
        self
    }

    /// Upstream `hasEntriesOnFocus`: the focus list, kept open across picks.
    pub fn entries_on_focus(mut self, on: bool) -> Self {
        self.base.entries_on_focus = on;
        self
    }

    /// Upstream `maxMenuItems`.
    pub fn max_menu_items(mut self, n: usize) -> Self {
        self.base = self.base.max_menu_items(n);
        self
    }

    /// Upstream `menuWidth`.
    pub fn menu_width(mut self, width: impl Into<Pixels>) -> Self {
        self.base.menu_width = Some(width.into());
        self
    }

    /// Upstream `minQueryLength`.
    pub fn min_query_length(mut self, n: usize) -> Self {
        self.base.min_query_length = n;
        self
    }

    /// Upstream `emptySearchText`.
    pub fn empty_search_text(mut self, text: impl Into<SharedString>) -> Self {
        self.base.empty_search_text = Some(text.into());
        self
    }

    /// Upstream `hasAutoFocus`.
    pub fn auto_focus(mut self, on: bool) -> Self {
        self.base.auto_focus = on;
        self
    }

    /// Upstream `debounceMs`.
    pub fn debounce(mut self, debounce: Duration) -> Self {
        self.base.debounce = debounce;
        self
    }

    /// Upstream `hasCreate`: a "Create" row for unmatched text, which
    /// `f` makes into an item.
    pub fn create(mut self, f: impl Fn(&str) -> T + 'static) -> Self {
        self.create = Some(Rc::new(f));
        self
    }

    /// Upstream `onChangeQuery`.
    pub fn on_change_query(
        mut self,
        f: impl Fn(SharedString, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.base = self.base.on_change_query(f);
        self
    }

    /// Upstream `startIcon`.
    pub fn start_icon(mut self, icon: impl IntoElement) -> Self {
        self.start_icon = Some(icon.into_any_element());
        self
    }

    /// Upstream `endContent`: at the row's end.
    pub fn end_content(mut self, content: impl IntoElement) -> Self {
        self.end_content = Some(content.into_any_element());
        self
    }

    /// Upstream `handleRef`.
    pub fn controller(mut self, controller: TokenizerController) -> Self {
        self.base.controller = Some(controller.0);
        self
    }

    /// Upstream `menu`.
    pub fn menu(mut self, menu: TypeaheadMenu) -> Self {
        self.base.menu = menu;
        self
    }

    /// Upstream `onOpenChange`.
    pub fn on_open_change(mut self, f: impl Fn(bool, &mut Window, &mut App) + 'static) -> Self {
        self.base = self.base.on_open_change(f);
        self
    }

    /// Upstream `onFocus`: focus entered the field.
    pub fn on_focus(mut self, f: impl Fn(&mut Window, &mut App) + 'static) -> Self {
        self.on_focus = Some(Rc::new(f));
        self
    }

    /// Upstream `onBlur`: focus left the field.
    pub fn on_blur(mut self, f: impl Fn(&mut Window, &mut App) + 'static) -> Self {
        self.on_blur = Some(Rc::new(f));
        self
    }

    /// Upstream `controlXstyle`.
    pub fn control_style(mut self, style: StyleRefinement) -> Self {
        self.control_style = style;
        self
    }
}

impl<T: Searchable> Styled for Tokenizer<T> {
    /// Upstream `containerXstyle`.
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

#[derive(Default)]
struct TokenizerState {
    /// Focus was within the field last frame.
    focused: bool,
    on_focus: Option<FocusFn>,
    on_blur: Option<FocusFn>,
}

impl<T: Searchable> RenderOnce for Tokenizer<T> {
    fn render(mut self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let id = self.base.id.clone();
        let disabled = self.disabled || group_disabled(cx);
        let place = group_place(cx);
        let group = place.and_then(|_| group_context(cx));
        let dir = use_direction(cx);
        let theme = current(window, cx);
        let size = resolve_size(self.size, ElementSize::Md, cx);
        let t18 = i18n(cx);
        let rem = window.rem_size();
        let state = window.use_keyed_state(
            ElementId::Name(format!("{id}-tokenizer").into()),
            cx,
            |_, _| TokenizerState::default(),
        );
        let group_focus = window
            .use_keyed_state(
                ElementId::Name(format!("{id}-tokenizer-group").into()),
                cx,
                |_, cx| cx.focus_handle().tab_stop(false),
            )
            .read(cx)
            .clone();
        let editor = window.use_keyed_state(
            ElementId::Name(format!("{id}-typeahead-input").into()),
            cx,
            |window, cx| {
                astryx_primitives::editing::TextEditState::reporting("", false, window, cx)
            },
        );
        let input_focus = editor.read(cx).focus_handle(cx);
        let engine = crate::typeahead::engine::engine_of::<T>(&id, window, cx);
        let value = self.value.clone();
        let at_limit = self.max_entries.is_some_and(|m| value.len() >= m);

        // onFocus and onBlur, as focus enters and leaves the field.
        let focused = group_focus.contains_focused(window, cx) || input_focus.is_focused(window);
        state.update(cx, |s, _| {
            s.on_focus = self.on_focus.clone();
            s.on_blur = self.on_blur.clone();
        });
        if focused != state.read(cx).focused {
            let report = state.update(cx, |s, _| {
                s.focused = focused;
                if focused {
                    s.on_focus.clone()
                } else {
                    s.on_blur.clone()
                }
            });
            if let Some(f) = report {
                window.defer(cx, move |window, cx| f(window, cx));
            }
        }

        let remove: RemoveFn<T> = {
            let (value, report, focus) =
                (value.clone(), self.on_change.clone(), input_focus.clone());
            Rc::new(move |item: &T, window: &mut Window, cx: &mut App| {
                if disabled {
                    return;
                }
                let next: Vec<T> = value
                    .iter()
                    .filter(|v| v.id() != item.id())
                    .cloned()
                    .collect();
                if let Some(f) = &report {
                    f(
                        next,
                        TokenizerChange {
                            item: item.clone(),
                            kind: TokenizerChangeKind::Remove,
                        },
                        window,
                        cx,
                    );
                }
                let text = i18n(cx).format(
                    "@astryx.tokenizer.tokenRemoved",
                    &[("label", item.label().as_ref().into())],
                );
                announce(text, Politeness::Polite, window, cx);
                window.focus(&focus, cx);
            })
        };

        // The search: what is chosen is never offered (FR13).
        self.base.exclude = value.iter().map(|v| v.id()).collect();
        self.base.keep_open_on_pick = self.base.entries_on_focus;
        self.base.focusable_disabled = disabled;
        if let Some(factory) = self.create.clone() {
            let query = engine.read(cx).query.trim().to_string();
            let (chosen_ids, chosen_labels): (Vec<SharedString>, Vec<String>) = value
                .iter()
                .map(|v| (v.id(), v.label().to_lowercase()))
                .unzip();
            let at_limit_now = at_limit;
            self.base.query_entries = Some(Rc::new(move |query: &str, results: &[T]| {
                let trimmed = query.trim();
                let lower = trimmed.to_lowercase();
                let exists = trimmed.is_empty()
                    || chosen_ids.iter().any(|i| i.as_ref() == trimmed)
                    || chosen_labels.contains(&lower)
                    || results.iter().any(|r| r.label().to_lowercase() == lower);
                if exists || at_limit_now {
                    return Vec::new();
                }
                vec![factory(trimmed)]
            }));
            self.base.entry_label = (!query.is_empty()).then(|| {
                t18.format(
                    "@astryx.tokenizer.create",
                    &[("value", query.as_str().into())],
                )
            });
        }
        {
            let (value, report, engine) = (value.clone(), self.on_change.clone(), engine.clone());
            let factory = self.create.clone();
            self.base.on_change = Some(Rc::new(
                move |item: Option<T>, window: &mut Window, cx: &mut App| {
                    let Some(item) = item else { return };
                    // A create row is one of the rows appended after the source's.
                    let created = factory.is_some() && {
                        let e = engine.read(cx);
                        let source_end = e.results.len() - e.entry_count;
                        e.results
                            .iter()
                            .position(|r| r.id() == item.id())
                            .is_some_and(|at| at >= source_end)
                    };
                    let item = match (&factory, created) {
                        (Some(f), true) => f(item.label().as_ref()),
                        _ => item,
                    };
                    let mut next = value.clone();
                    next.push(item.clone());
                    let kind = if created {
                        TokenizerChangeKind::Create
                    } else {
                        TokenizerChangeKind::Add
                    };
                    let label = item.label();
                    if let Some(f) = &report {
                        f(next, TokenizerChange { item, kind }, window, cx);
                    }
                    let text = i18n(cx).format(
                        "@astryx.tokenizer.tokenAdded",
                        &[("label", label.as_ref().into())],
                    );
                    announce(text, Politeness::Polite, window, cx);
                },
            ));
        }
        // Backspace in the empty field removes the last token.
        {
            let (value, engine, remove) = (value.clone(), engine.clone(), remove.clone());
            self.base.on_key_down = Some(Rc::new(
                move |event: &KeyDownEvent, window: &mut Window, cx: &mut App| {
                    if event.keystroke.key == "backspace"
                        && engine.read(cx).query.is_empty()
                        && let Some(last) = value.last()
                    {
                        remove(last, window, cx);
                        return true;
                    }
                    false
                },
            ));
        }
        let name: SharedString = match &group {
            Some(group) => format!("{} {}", group.label, self.label).into(),
            None => self.label.clone(),
        };
        self.base.label = Some(name.clone());
        let mut described = described_by(
            self.description.as_ref(),
            self.status
                .as_ref()
                .filter(|_| self.status_placement != StatusPlacement::Tooltip),
            disabled.then_some(self.disabled_message.as_ref()).flatten(),
            &t18,
        );
        if let Some(group) = &group {
            described = group.described.iter().cloned().chain(described).collect();
        }
        if !described.is_empty() {
            self.base.description = Some(described.join(" ").into());
        }
        self.base.required = crate::field::resolve(self.required, self.optional, cx).required;
        let status = self.status.as_ref().map(|s| s.status);
        self.base.invalid = status == Some(StatusType::Error);
        self.base.size = size;
        if !value.is_empty() {
            self.base.placeholder = Some(SharedString::default());
        }
        // FR15: at the limit the list closes.
        if at_limit && engine.read(cx).open {
            let report = engine.update(cx, |e, _| {
                e.open = false;
                e.highlight = None;
                e.on_open_change.clone()
            });
            if let Some(f) = report {
                window.defer(cx, move |window, cx| f(false, window, cx));
            }
        }
        let parts = self.base.parts(window, cx);

        // The tokens.
        let tokens = |window: &mut Window, cx: &mut App| -> Vec<AnyElement> {
            value
                .iter()
                .map(|item| {
                    let act: ActFn = {
                        let (remove, item) = (remove.clone(), item.clone());
                        Rc::new(move |window: &mut Window, cx: &mut App| remove(&item, window, cx))
                    };
                    match &self.render_token {
                        Some(f) => f(item, act, window, cx),
                        None => {
                            let mut token = Token::new(
                                ElementId::Name(format!("{id}-token-{}", item.id()).into()),
                                item.label(),
                            );
                            if !disabled {
                                token = token.on_remove(move |_, window, cx| act(window, cx));
                            }
                            token.into_any_element()
                        }
                    }
                })
                .collect()
        };
        let chrome = Chrome {
            id: ElementId::Name(format!("{id}-control").into()),
            target: "tokenizer",
            variant: None,
            size,
            status,
            disabled,
            read_only: false,
            focused,
            focus_visible: is_focus_visible(&input_focus, window),
            row: false,
            place,
            dir,
        };
        let part_override = theme.for_part("tokenizer", &chrome.props());
        let part = part_override.clone().unwrap_or_else(|| theme.clone());
        let t = part.tokens();
        let folded =
            !focused && self.token_overflow != TokenOverflowBehavior::None && !value.is_empty();
        let mut surface = control_box(&chrome, &part, window, cx)
            .track_focus(&group_focus)
            .role(Role::Group)
            .aria_label(name.clone())
            .items_center()
            .gap(t.spacing.s1)
            .px(t.spacing.s1)
            .py(t.spacing.s1.to_pixels(rem) - px(1.))
            .cursor_text();
        if place.is_none() {
            surface = surface.min_h(crate::button::element_height(size, t));
        }
        surface = if folded || place.is_some() {
            surface.overflow_hidden()
        } else {
            surface.flex_wrap()
        };
        surface.style().refine(&self.control_style);
        if !disabled {
            let focus = input_focus.clone();
            surface = surface.on_mouse_down(gpui::MouseButton::Left, move |_, window, cx| {
                window.focus(&focus, cx);
            });
        }
        if let Some(icon) = self.start_icon.take() {
            surface = surface.child(
                div()
                    .flex()
                    .flex_none()
                    .items_center()
                    .text_color(t.color.icon_secondary)
                    .child(provide(IconDefaultSize(IconSize::Sm), icon)),
            );
        }
        if folded {
            // Blurred: one row of tokens and a count of the rest.
            let count_label = t18.clone();
            let mut row = OverflowList::new(ElementId::Name(format!("{id}-folded").into()))
                .gap(crate::layout::SpacingStep::S1)
                .overflow_renderer(move |hidden, _, _| {
                    Badge::new(count_label.format(
                        "@astryx.tokenizer.overflowCount",
                        &[("count", astryx_i18n::Arg::Num(hidden.len() as f64))],
                    ))
                    .into_any_element()
                });
            for item in &value {
                let label = item.label();
                let token_id = ElementId::Name(format!("{id}-folded-{}", item.id()).into());
                row = row.keyed_item(item.id(), move |_, _| {
                    Token::new(token_id.clone(), label.clone()).into_any_element()
                });
            }
            let folded_selector = format!("{id}-folded");
            surface = surface.child(
                div()
                    .debug_selector(move || folded_selector)
                    .flex_1()
                    .min_w_0()
                    .child(row),
            );
        } else {
            surface = surface.children(tokens(window, cx));
        }
        // FR1: the field stays mounted, collapsed at the limit.
        let input = if at_limit || folded {
            div()
                .w(px(0.))
                .h(px(0.))
                .overflow_hidden()
                .opacity(0.)
                .child(parts.input)
                .into_any_element()
        } else {
            div()
                .flex_1()
                .min_w(px(60.))
                .flex()
                .child(parts.input)
                .into_any_element()
        };
        surface = surface.child(input);
        // The end lane.
        let show_clear = self.clear && !value.is_empty() && !disabled;
        if parts.busy || show_clear || self.end_content.is_some() || status.is_some() {
            let mut end = div()
                .flex()
                .flex_none()
                .items_center()
                .gap(t.spacing.s1)
                .ml_auto();
            if parts.busy {
                end = end.child(
                    div()
                        .debug_selector({
                            let s = format!("{id}-busy");
                            move || s
                        })
                        .child(
                            Spinner::new(ElementId::Name(format!("{id}-spinner").into()))
                                .size(SpinnerSize::Sm),
                        ),
                );
            }
            if show_clear {
                let (value, report, focus) =
                    (value.clone(), self.on_change.clone(), input_focus.clone());
                end = end.child(
                    div()
                        .debug_selector({
                            let s = format!("{id}-clear");
                            move || s
                        })
                        .child(
                            IconButton::new(
                                ElementId::Name(format!("{id}-clear").into()),
                                t18.t("@astryx.tokenizer.clearAll"),
                                Icon::new("close")
                                    .size(IconSize::Sm)
                                    .color(IconColor::Secondary),
                            )
                            .variant(ButtonVariant::Ghost)
                            .size(ElementSize::Sm)
                            .tooltip(t18.t("@astryx.tokenizer.clearAll"))
                            .h(px(20.))
                            .on_click(move |_, window, cx| {
                                cx.stop_propagation();
                                let Some(last) = value.last().cloned() else {
                                    return;
                                };
                                if let Some(f) = &report {
                                    f(
                                        Vec::new(),
                                        TokenizerChange {
                                            item: last,
                                            kind: TokenizerChangeKind::Remove,
                                        },
                                        window,
                                        cx,
                                    );
                                }
                                announce(
                                    i18n(cx).t("@astryx.tokenizer.allRemoved"),
                                    Politeness::Polite,
                                    window,
                                    cx,
                                );
                                window.focus(&focus, cx);
                            }),
                        ),
                );
            }
            if let Some(content) = self.end_content.take() {
                end = end.child(content);
            }
            if let Some(status) = status
                .filter(|_| self.status_placement != StatusPlacement::Detached && place.is_none())
            {
                let message = self.status.as_ref().and_then(|s| s.message.clone());
                end = end.child(status_indicator(
                    &id,
                    status,
                    self.status_placement,
                    message.as_ref(),
                    &part,
                    window,
                    cx,
                ));
            }
            surface = surface.child(end);
        }
        let surface = match part_override {
            Some(part) => scope(part, surface).into_any_element(),
            None => surface.into_any_element(),
        };
        let control = (parts.wrap)(surface, window, cx);
        let control = crate::overlay::disabled_reason(
            &id,
            disabled,
            self.disabled_message.as_ref(),
            true,
            control,
        );
        let control = if self.token_overflow == TokenOverflowBehavior::UnfocusedLayer && focused {
            // FR3: open, the field draws over its placeholder in a layer.
            let height = window.use_keyed_state(
                ElementId::Name(format!("{id}-tokenizer-row").into()),
                cx,
                |_, _| px(0.),
            );
            let placeholder = div()
                .w_full()
                .h(crate::button::element_height(size, t).to_pixels(rem));
            let _ = height;
            anchored_layer(
                ElementId::Name(format!("{id}-tokenizer-layer").into()),
                placeholder,
            )
            .light_dismiss(false)
            .match_anchor_width(true)
            .position_with(|anchor, _, _, _| LayerPosition {
                origin: point(anchor.origin.x, anchor.origin.y),
                side: Placement::Below,
            })
            .surface(
                div().w_full().child(control),
                LayerOptions {
                    placement: Placement::Below,
                    align: LayerAlign::Start,
                    offset: px(0.),
                    margin: px(0.),
                    rtl: dir.is_rtl(),
                    priority: 1,
                },
                |_, _| {},
            )
            .into_any_element()
        } else {
            control
        };
        if place.is_some() {
            return inside_member(control).into_any_element();
        }
        let mut field = Field::new(id, self.label)
            .description_opt(self.description)
            .label_hidden(self.label_hidden)
            .disabled(disabled)
            .optional(self.optional)
            .required(self.required)
            .status_placement(self.status_placement);
        if let Some(status) = self.status {
            field = field.status(status);
        }
        if let Some(tooltip) = self.label_tooltip {
            field = field.label_tooltip(tooltip);
        }
        if let Some(width) = self.width {
            field = field.width(width);
        }
        field.style().refine(&self.style);
        field.child(control).into_any_element()
    }
}
