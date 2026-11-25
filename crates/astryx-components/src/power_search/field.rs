//! The PowerSearch field, upstream `PowerSearch.tsx` (popover
//! presentation): a Tokenizer of filter tokens over the main rows, and the
//! filter editor in a popover below it.

use std::collections::HashMap;
use std::rc::Rc;

use astryx_i18n::{Arg, current as i18n};
use astryx_primitives::{ElementSize, LayerAlign, Politeness, announce};
use astryx_theme::current;
use gpui::{
    AnyElement, App, Bounds, ElementId, Entity, IntoElement, Length, ParentElement, Pixels,
    RenderOnce, SharedString, StyleRefinement, Styled, Window, div, prelude::*, px, relative,
};

use super::editor::{EditorMode, PowerSearchFilterEditor};
use super::format::{
    format_filter_value, format_token_label, format_token_text, format_token_value,
    token_text_keeps_parts,
};
use super::model::{
    FilterValue, OperatorValue, PartialFilter, PowerSearchChangeType, PowerSearchConfig,
    PowerSearchField, PowerSearchFilter, PowerSearchOperator,
};
use super::source::{PowerSearchItem, RowData, field_rows, typed_rows};
use crate::field::{InputStatus, StatusPlacement};
use crate::icon::{Icon, IconColor, IconSize};
use crate::layout::SpacingStep;
use crate::overlay::{Popover, PopoverController, PopoverRole};
use crate::people::Avatar;
use crate::search::{SearchAnswer, SearchContext, SearchSource};
use crate::token::Token;
use crate::tokenizer::{
    TokenOverflowBehavior, Tokenizer, TokenizerChangeKind, TokenizerController,
};
use crate::typeahead::TypeaheadItem;

type ChangeFn =
    Rc<dyn Fn(Vec<PowerSearchFilter>, PowerSearchChangeType, usize, &mut Window, &mut App)>;
type ActFn = Rc<dyn Fn(&mut Window, &mut App)>;
/// Saves a filter, or deletes it with none.
pub type SaveFn = Rc<dyn Fn(Option<PowerSearchFilter>, &mut Window, &mut App)>;
type SetEditingFn = Rc<dyn Fn(Editing, &mut Window, &mut App)>;
type IndexFn = Rc<dyn Fn(usize, &mut Window, &mut App)>;
type TokenOverrideFn = Rc<dyn Fn(PowerSearchTokenProps, &mut Window, &mut App) -> AnyElement>;
type EditorOverrideFn = Rc<dyn Fn(PowerSearchEditorProps, &mut Window, &mut App) -> AnyElement>;

/// Upstream's `DEFAULT_MAX_SEARCH_RESULTS`.
pub const DEFAULT_MAX_SEARCH_RESULTS: usize = 10;
/// Upstream's `MAX_BROWSE_MENU_ITEMS`.
const MAX_BROWSE_MENU_ITEMS: usize = 1000;
/// The editor's width range (FR1, FR2).
const EDITOR_MIN: f32 = 400.;
const EDITOR_MAX: f32 = 720.;

/// What a token override gets (upstream `PowerSearchTokenProps`).
#[derive(Clone)]
pub struct PowerSearchTokenProps {
    pub config: Rc<PowerSearchConfig>,
    pub filter: PowerSearchFilter,
    pub field: PowerSearchField,
    pub operator: PowerSearchOperator,
    pub max_length: usize,
    pub on_click: Option<ActFn>,
    pub on_remove: Option<ActFn>,
    pub disabled: bool,
    /// Upstream `timezoneID`: the zone date values read in (FR25, FR26).
    pub timezone_id: Option<SharedString>,
}

/// What an editor override gets (upstream `PowerSearchEditorProps`).
#[derive(Clone)]
pub struct PowerSearchEditorProps {
    pub config: Rc<PowerSearchConfig>,
    pub filter: PartialFilter,
    pub mode: EditorMode,
    pub on_save: SaveFn,
    pub on_cancel: ActFn,
    pub save_label: SharedString,
    pub read_only: bool,
    /// Upstream `timezoneID`.
    pub timezone_id: Option<SharedString>,
}

/// One value type's overrides (upstream `PowerSearchComponentOverride`).
#[derive(Clone, Default)]
pub struct PowerSearchComponentOverride {
    pub token: Option<TokenOverrideFn>,
    pub editor: Option<EditorOverrideFn>,
}

impl PowerSearchComponentOverride {
    pub fn token(
        mut self,
        f: impl Fn(PowerSearchTokenProps, &mut Window, &mut App) -> AnyElement + 'static,
    ) -> Self {
        self.token = Some(Rc::new(f));
        self
    }

    pub fn editor(
        mut self,
        f: impl Fn(PowerSearchEditorProps, &mut Window, &mut App) -> AnyElement + 'static,
    ) -> Self {
        self.editor = Some(Rc::new(f));
        self
    }
}

/// Upstream `resultCount`: a count the catalog words, or text.
#[derive(Clone, Debug, PartialEq)]
pub enum ResultCount {
    Count(usize),
    Text(SharedString),
}

impl From<usize> for ResultCount {
    fn from(n: usize) -> Self {
        ResultCount::Count(n)
    }
}

impl From<&'static str> for ResultCount {
    fn from(text: &'static str) -> Self {
        ResultCount::Text(text.into())
    }
}

impl From<SharedString> for ResultCount {
    fn from(text: SharedString) -> Self {
        ResultCount::Text(text)
    }
}

/// Focuses and blurs the field from code, upstream `PowerSearchHandle`.
/// Make it with the PowerSearch's id in the render that draws it.
#[derive(Clone)]
pub struct PowerSearchController {
    tokenizer: TokenizerController,
}

impl PowerSearchController {
    pub fn new(id: impl Into<ElementId>, window: &mut Window, cx: &mut App) -> Self {
        let id: ElementId = id.into();
        PowerSearchController {
            tokenizer: TokenizerController::new(
                ElementId::Name(format!("{id}-field").into()),
                window,
                cx,
            ),
        }
    }

    /// Upstream `focusTypeahead()`.
    pub fn focus_typeahead(&self, window: &mut Window, cx: &mut App) {
        self.tokenizer.focus(window, cx)
    }

    /// Upstream `blurTypeahead()`.
    pub fn blur_typeahead(&self, window: &mut Window, cx: &mut App) {
        self.tokenizer.blur(window, cx)
    }
}

/// The editor popover's state (upstream `PopoverState`).
#[derive(Clone, Debug, PartialEq)]
enum Editing {
    Idle,
    Adding(PartialFilter),
    Edit(usize, PartialFilter),
}

struct FieldState {
    editing: Editing,
    /// Counts opens, so each starts a fresh draft.
    opens: u64,
    outer: Option<Bounds<Pixels>>,
    input: Option<Bounds<Pixels>>,
    tokens: HashMap<usize, Bounds<Pixels>>,
    /// The edge a capped editor keeps for one open (FR6).
    latched: Option<LayerAlign>,
    /// A "More" row's typed text, for the editor it opens (FR29).
    opening: Option<SharedString>,
    announced: Option<Option<SharedString>>,
}

/// The main field's source: the rows of `source::typed_rows`, then FR28's
/// search-source rows.
struct MainSource {
    config: Rc<PowerSearchConfig>,
    max_results: usize,
    max_items: usize,
    /// The value sources a query is waiting on, to cancel when superseded.
    inflight: Rc<std::cell::RefCell<Vec<super::model::ValueSource>>>,
}

impl SearchSource<PowerSearchItem> for MainSource {
    fn search(
        &self,
        query: &str,
        context: &SearchContext<PowerSearchItem>,
        cx: &mut App,
    ) -> SearchAnswer<PowerSearchItem> {
        let rows = typed_rows(&self.config, query, self.max_results, cx);
        let source_fields: Vec<usize> = if super::source::is_value_only(&self.config, query) {
            (0..self.config.fields.len())
                .filter(|i| super::source::is_source_field(&self.config.fields[*i], query))
                .collect()
        } else {
            Vec::new()
        };
        if source_fields.is_empty() {
            return rows.into();
        }
        // The rows that need no source show at once (FR28).
        context.update(rows.clone(), cx);
        let (config, query, context, max_items, inflight) = (
            self.config.clone(),
            query.to_string(),
            context.clone(),
            self.max_items,
            self.inflight.clone(),
        );
        SearchAnswer::Pending(cx.spawn(async move |cx| {
            // The editor's own wait before it searches the same source.
            cx.background_executor()
                .timer(super::value_editor::SOURCE_DEBOUNCE)
                .await;
            let mut all = rows;
            let mut answers: Vec<(usize, Vec<crate::search::SearchItem>)> = Vec::new();
            for index in source_fields {
                let field = config.fields[index].clone();
                for op in field.operators.iter().filter(|o| o.value_only_match) {
                    let Some(source) = op.value.search_source().cloned() else {
                        continue;
                    };
                    if super::source::takes_free_text(&op.value) {
                        continue;
                    }
                    let key = super::source::source_key(&source);
                    if answers.iter().any(|(k, _)| *k == key) {
                        continue;
                    }
                    inflight.borrow_mut().push(source.clone());
                    let quiet = SearchContext::new(|_, _| {});
                    let answer = cx.update(|cx| source.search(&query, &quiet, cx));
                    let items = match answer {
                        SearchAnswer::Ready(items) => items,
                        SearchAnswer::Pending(task) => task.await.unwrap_or_default(),
                    };
                    answers.push((key, items));
                }
                // This field's rows join, after every field before it.
                let added = cx.update(|cx| {
                    let t18 = i18n(cx);
                    let content = super::source::content_default(&config, &query, cx);
                    let content = content.as_ref().map(|(f, o)| (f.as_ref(), o.as_ref()));
                    let rows = super::source::value_only_rows_with(
                        &field, &query, content, &answers, max_items, cx,
                    );
                    super::source::cap_value_rows(&field, rows, &query, &t18)
                });
                for row in added {
                    if !all.iter().any(|r| r.id == row.id) {
                        all.push(row);
                    }
                }
                let shown = all.clone();
                cx.update(|cx| context.update(shown, cx));
            }
            inflight.borrow_mut().clear();
            Ok(all)
        }))
    }

    fn cancel(&self) {
        for source in self.inflight.borrow_mut().drain(..) {
            source.cancel();
        }
    }

    fn bootstrap(&self, _: &mut App) -> SearchAnswer<PowerSearchItem> {
        field_rows(&self.config).into()
    }
}

/// Upstream `commitsWithoutEditor`: an `empty` value with no editor override.
fn commits_without_editor(
    op: Option<&PowerSearchOperator>,
    components: &HashMap<&'static str, PowerSearchComponentOverride>,
) -> bool {
    matches!(op.map(|o| &o.value), Some(OperatorValue::Empty))
        && components.get("empty").is_none_or(|c| c.editor.is_none())
}

/// The editor's width and edge for one open (FR1-FR6): the outer width
/// clamped to 400-720 px (the floor yields to a narrow window), at the
/// outer start unless capped; capped, at the outer edge nearer the opening
/// control.
pub(crate) fn editor_geometry(
    outer: Bounds<Pixels>,
    opener: Option<Bounds<Pixels>>,
    viewport: Pixels,
    rtl: bool,
) -> (Pixels, LayerAlign) {
    let width = outer
        .size
        .width
        .max(px(EDITOR_MIN).min(viewport))
        .min(px(EDITOR_MAX));
    if outer.size.width <= px(EDITOR_MAX) {
        return (width, LayerAlign::Start);
    }
    let Some(opener) = opener else {
        return (width, LayerAlign::Start);
    };
    let to_left = opener.left() - outer.left();
    let to_right = outer.right() - opener.right();
    let nearer_left = to_left <= to_right;
    // Start and end are logical: under RTL the start is the right edge.
    let align = match (nearer_left, rtl) {
        (true, false) | (false, true) => LayerAlign::Start,
        _ => LayerAlign::End,
    };
    (width, align)
}

/// A structured filter bar, upstream `PowerSearch`: each token is a
/// filter of a field, an operator and a value.
///
/// ```ignore
/// PowerSearch::new("search", config.clone(), filters.clone(), |filters, _, _, _, _| { /* store */ })
/// ```
#[derive(IntoElement)]
pub struct PowerSearch {
    id: ElementId,
    config: Rc<PowerSearchConfig>,
    filters: Vec<PowerSearchFilter>,
    on_change: ChangeFn,
    label: Option<SharedString>,
    label_hidden: bool,
    placeholder: Option<SharedString>,
    auto_focus: bool,
    clear: bool,
    read_only: bool,
    disabled: bool,
    disabled_message: Option<SharedString>,
    status: Option<InputStatus>,
    status_placement: Option<StatusPlacement>,
    start_icon: Option<AnyElement>,
    max_token_length: usize,
    max_operator_menu_items: Option<usize>,
    max_search_results: usize,
    menu_width: Option<Pixels>,
    save_label: Option<SharedString>,
    token_overflow: Option<TokenOverflowBehavior>,
    end_content: Option<AnyElement>,
    result_count: Option<ResultCount>,
    controller: Option<PowerSearchController>,
    size: Option<ElementSize>,
    components: HashMap<&'static str, PowerSearchComponentOverride>,
    on_focus: Option<ActFn>,
    on_blur: Option<ActFn>,
    presentation: crate::overlay::PopoverPresentation,
    timezone_id: Option<SharedString>,
    style: StyleRefinement,
}

impl PowerSearch {
    /// Upstream `config`, `filters` and `onChange` (the filters after a change,
    /// what it did, and the index it touched).
    pub fn new(
        id: impl Into<ElementId>,
        config: Rc<PowerSearchConfig>,
        filters: Vec<PowerSearchFilter>,
        on_change: impl Fn(Vec<PowerSearchFilter>, PowerSearchChangeType, usize, &mut Window, &mut App)
        + 'static,
    ) -> Self {
        PowerSearch {
            id: id.into(),
            config,
            filters,
            on_change: Rc::new(on_change),
            label: None,
            label_hidden: true,
            placeholder: None,
            auto_focus: false,
            clear: true,
            read_only: false,
            disabled: false,
            disabled_message: None,
            status: None,
            status_placement: None,
            start_icon: None,
            max_token_length: 40,
            max_operator_menu_items: None,
            max_search_results: DEFAULT_MAX_SEARCH_RESULTS,
            menu_width: None,
            save_label: None,
            token_overflow: None,
            end_content: None,
            result_count: None,
            controller: None,
            size: None,
            components: HashMap::new(),
            on_focus: None,
            on_blur: None,
            presentation: crate::overlay::PopoverPresentation::Adaptive,
            timezone_id: None,
            style: StyleRefinement::default(),
        }
    }

    astryx_primitives::setters! {
        /// Upstream `label`: "Search" by default.
        ?label: SharedString,
        /// Upstream `isLabelHidden`: hidden by default.
        label_hidden: bool,
        /// Upstream `placeholder`: shown while there is no filter.
        ?placeholder: SharedString,
        /// Upstream `hasAutoFocus`.
        auto_focus: bool,
        /// Upstream `hasClear`: on by default; never while read-only.
        clear: bool,
        /// Upstream `isReadOnly`.
        read_only: bool,
        /// Upstream `isDisabled`.
        disabled: bool,
        /// Upstream `disabledMessage`.
        ?disabled_message: SharedString,
        /// Upstream `status`.
        ?status: InputStatus,
        /// Upstream `statusVariant`.
        ?status_placement: StatusPlacement,
        /// Upstream `maxTokenLength`: 40 by default.
        max_token_length: usize,
        /// Upstream `maxSearchResults`: 10 by default.
        max_search_results: usize,
        /// Upstream `popoverSaveButtonLabel`: "Apply" by default.
        ?save_label: SharedString,
        /// Upstream `tokenOverflowBehavior`.
        ?token_overflow: TokenOverflowBehavior,
        /// Upstream `size`.
        ?size: ElementSize,
        /// Upstream `presentation`: adaptive by default.
        presentation: crate::overlay::PopoverPresentation,
    }

    /// Upstream `timezoneID`: the zone a date-only value reads and writes in.
    pub fn timezone_id(mut self, zone: impl Into<SharedString>) -> Self {
        self.timezone_id = Some(zone.into());
        self
    }

    /// Upstream `maxOperatorMenuItems`.
    pub fn max_operator_menu_items(mut self, n: usize) -> Self {
        self.max_operator_menu_items = Some(n);
        self
    }

    /// Upstream `menuWidth`: the main menu's width, never under the field's.
    pub fn menu_width(mut self, width: impl Into<Pixels>) -> Self {
        self.menu_width = Some(width.into());
        self
    }

    /// Upstream `startIcon`.
    pub fn start_icon(mut self, icon: impl IntoElement) -> Self {
        self.start_icon = Some(icon.into_any_element());
        self
    }

    /// Upstream `endContent`.
    pub fn end_content(mut self, content: impl IntoElement) -> Self {
        self.end_content = Some(content.into_any_element());
        self
    }

    /// Upstream `resultCount`: at the field's end, announced when it changes.
    pub fn result_count(mut self, count: impl Into<ResultCount>) -> Self {
        self.result_count = Some(count.into());
        self
    }

    /// Upstream `handleRef`.
    pub fn controller(mut self, controller: PowerSearchController) -> Self {
        self.controller = Some(controller);
        self
    }

    /// Upstream `components`: a value type's (`"string"`, `"empty"`, ...)
    /// own token or editor.
    pub fn component(
        mut self,
        value_type: &'static str,
        overrides: PowerSearchComponentOverride,
    ) -> Self {
        self.components.insert(value_type, overrides);
        self
    }

    /// Upstream `onFocus`.
    pub fn on_focus(mut self, f: impl Fn(&mut Window, &mut App) + 'static) -> Self {
        self.on_focus = Some(Rc::new(f));
        self
    }

    /// Upstream `onBlur`.
    pub fn on_blur(mut self, f: impl Fn(&mut Window, &mut App) + 'static) -> Self {
        self.on_blur = Some(Rc::new(f));
        self
    }
}

impl Styled for PowerSearch {
    /// Upstream `xstyle`: the field's box.
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl RenderOnce for PowerSearch {
    fn render(mut self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = current(window, cx);
        let t = theme.tokens();
        let t18 = i18n(cx);
        let rtl = astryx_i18n::use_direction(cx).is_rtl();
        let config = self.config.clone();
        let filters = self.filters.clone();
        let state: Entity<FieldState> = window.use_keyed_state(
            ElementId::Name(format!("{}-ps", self.id).into()),
            cx,
            |_, _| FieldState {
                editing: Editing::Idle,
                opens: 0,
                outer: None,
                input: None,
                tokens: HashMap::new(),
                latched: None,
                opening: None,
                announced: None,
            },
        );
        let label = self
            .label
            .clone()
            .unwrap_or_else(|| t18.t("@astryx.powersearch.label"));
        let placeholder = self
            .placeholder
            .clone()
            .unwrap_or_else(|| t18.t("@astryx.powersearch.placeholder"));
        let save_label = self
            .save_label
            .clone()
            .unwrap_or_else(|| t18.t("@astryx.powersearch.editor.apply"));
        let tokenizer_controller = match &self.controller {
            Some(c) => c.tokenizer.clone(),
            None => TokenizerController::new(
                ElementId::Name(format!("{}-field", self.id).into()),
                window,
                cx,
            ),
        };
        let popover_id = ElementId::Name(format!("{}-editor-popover", self.id).into());
        let popover = PopoverController::new(popover_id.clone(), window, cx);
        let (read_only, disabled) = (self.read_only, self.disabled);
        let on_change = self.on_change.clone();
        let components = Rc::new(self.components.clone());
        let timezone_id = self.timezone_id.clone();
        // FR10: adaptive resolves to the sheet on compact touch screens; an
        // open flow keeps it until it closes.
        let sheet_state = super::sheet::flow_state(&self.id, window, cx);
        let sheet = crate::overlay::presents_as_sheet(self.presentation, window)
            || sheet_state.read(cx).open;

        // Opening and closing the editor.
        let set_editing: SetEditingFn = {
            let (state, popover, tokenizer) =
                (state.clone(), popover.clone(), tokenizer_controller.clone());
            Rc::new(move |editing: Editing, window: &mut Window, cx: &mut App| {
                let idle = editing == Editing::Idle;
                state.update(cx, |s, _| {
                    s.editing = editing;
                    if idle {
                        s.opening = None;
                    } else {
                        s.opens += 1;
                        s.latched = None;
                    }
                });
                if idle {
                    popover.hide(window, cx);
                    tokenizer.focus(window, cx);
                } else {
                    let (popover, tokenizer) = (popover.clone(), tokenizer.clone());
                    window.defer(cx, move |window, cx| {
                        popover.show(window, cx);
                        tokenizer.blur(window, cx);
                    });
                }
                window.refresh();
            })
        };

        // The token text (FR24): label, value, and the whole.
        let max_token_length = self.max_token_length;
        let token_text = {
            let (config, zone) = (config.clone(), timezone_id.clone());
            move |filter: &PowerSearchFilter, cx: &App| -> (String, Option<String>, String) {
                let t18 = i18n(cx);
                let field = config.find_field(&filter.field);
                let op = config.find_operator(&filter.field, &filter.operator);
                let field_label = field.map_or(filter.field.to_string(), |f| f.label.to_string());
                let op_label = op.map(|o| o.label_text(cx).to_string()).unwrap_or_default();
                let label = format_token_label(&field_label, &op_label, &t18);
                let budget = max_token_length
                    .saturating_sub(field_label.chars().count() + op_label.chars().count())
                    .max(10);
                let value = op.and_then(|o| {
                    format_token_value(&o.value, &filter.value, budget, &t18, zone.as_deref())
                });
                let full = match &value {
                    Some(v) => format_token_text(&label, v, &t18),
                    None => label.clone(),
                };
                (label, value, full)
            }
        };

        // FR19: the result count, announced when it changes after mount.
        let count_text: Option<SharedString> = self.result_count.as_ref().map(|c| match c {
            ResultCount::Count(n) => t18.format(
                "@astryx.powersearch.resultCount",
                &[("count", Arg::Num(*n as f64))],
            ),
            ResultCount::Text(text) => text.clone(),
        });
        match state.read(cx).announced.clone() {
            None => state.update(cx, |s, _| s.announced = Some(count_text.clone())),
            Some(previous) if previous != count_text => {
                state.update(cx, |s, _| s.announced = Some(count_text.clone()));
                if let Some(text) = &count_text {
                    announce(text.clone(), Politeness::Polite, window, cx);
                }
            }
            _ => {}
        }
        if sheet {
            let count_text: Option<SharedString> = self.result_count.as_ref().map(|c| match c {
                ResultCount::Count(n) => t18.format(
                    "@astryx.powersearch.resultCount",
                    &[("count", Arg::Num(*n as f64))],
                ),
                ResultCount::Text(text) => text.clone(),
            });
            let end_lane = match (count_text, self.end_content.take()) {
                (None, None) => None,
                (count, content) => Some(
                    div()
                        .flex()
                        .items_center()
                        .gap(t.spacing.s2)
                        .children(count.map(|c| {
                            div()
                                .text_size(t.text.supporting.size)
                                .line_height(relative(t.text.supporting.leading))
                                .text_color(t.color.text_secondary)
                                .whitespace_nowrap()
                                .child(c)
                        }))
                        .children(content)
                        .into_any_element(),
                ),
            };
            let token_override: super::sheet::TokenOverrideFn = {
                let (config, components, zone) =
                    (config.clone(), components.clone(), timezone_id.clone());
                let max = self.max_token_length;
                Rc::new(
                    move |filter: &PowerSearchFilter, window: &mut Window, cx: &mut App| {
                        let field = config.find_field(&filter.field)?.clone();
                        let op = field.find_operator(&filter.operator)?.clone();
                        let token = components.get(op.value.kind())?.token.clone()?;
                        Some(token(
                            PowerSearchTokenProps {
                                config: config.clone(),
                                filter: filter.clone(),
                                field,
                                operator: op,
                                max_length: max,
                                on_click: None,
                                on_remove: None,
                                disabled,
                                timezone_id: zone.clone(),
                            },
                            window,
                            cx,
                        ))
                    },
                )
            };
            let trigger = super::sheet::trigger_field(
                super::sheet::TriggerProps {
                    timezone_id: timezone_id.clone(),
                    id: self.id.clone(),
                    config: config.clone(),
                    filters: filters.clone(),
                    label: label.clone(),
                    label_hidden: self.label_hidden,
                    placeholder: placeholder.clone(),
                    disabled,
                    read_only,
                    disabled_message: self.disabled_message.clone(),
                    size: self.size,
                    max_token_length: self.max_token_length,
                    start_icon: self.start_icon.take(),
                    end_lane,
                    status: self.status.clone(),
                    status_placement: self.status_placement,
                    token_override,
                },
                window,
                cx,
            );
            let flow = super::sheet::flow(
                super::sheet::SheetProps {
                    timezone_id: timezone_id.clone(),
                    id: self.id.clone(),
                    config: config.clone(),
                    filters: filters.clone(),
                    on_change: on_change.clone(),
                    read_only,
                    disabled,
                    clear: self.clear,
                    max_menu_items: self.max_operator_menu_items,
                    max_search_results: self.max_search_results,
                    save_label: self.save_label.clone(),
                    placeholder: placeholder.clone(),
                },
                window,
                cx,
            );
            let mut root = div().w_full().child(trigger).child(flow);
            root.style().refine(&self.style);
            return root.into_any_element();
        }

        // The Tokenizer's items: one per filter.
        let items: Vec<PowerSearchItem> = filters
            .iter()
            .enumerate()
            .map(|(index, filter)| {
                let op = config.find_operator(&filter.field, &filter.operator);
                let field_label = config
                    .find_field(&filter.field)
                    .map_or(filter.field.clone(), |f| f.label.clone());
                let op_label = op.map(|o| o.label_text(cx).to_string()).unwrap_or_default();
                let label = format_token_label(&field_label, &op_label, &t18);
                let value = op
                    .map(|o| {
                        format_filter_value(
                            &o.value,
                            &filter.value,
                            max_token_length,
                            &t18,
                            timezone_id.as_deref(),
                        )
                    })
                    .unwrap_or_default();
                let display = if value.is_empty() {
                    label
                } else {
                    format_token_text(&label, &value, &t18)
                };
                PowerSearchItem {
                    id: format!("filter-{index}-{}-{}", filter.field, filter.operator).into(),
                    label: display.into(),
                    data: RowData {
                        field_key: filter.field.clone(),
                        operator_key: Some(filter.operator.clone()),
                        filter_value: Some(filter.value.clone()),
                        group: None,
                        more_query: None,
                        filter_index: Some(index),
                    },
                }
            })
            .collect();

        // Picking a row, removing tokens (Clear all keeps read-only ones).
        let handle_change = {
            let (config, filters, on_change, set_editing) = (
                config.clone(),
                filters.clone(),
                on_change.clone(),
                set_editing.clone(),
            );
            let opening_state = state.clone();
            move |kept: Vec<PowerSearchItem>,
                  change: crate::tokenizer::TokenizerChange<PowerSearchItem>,
                  window: &mut Window,
                  cx: &mut App| {
                match change.kind {
                    TokenizerChangeKind::Add | TokenizerChangeKind::Create => {
                        let data = change.item.data.clone();
                        let Some(field) = config.find_field(&data.field_key) else {
                            return;
                        };
                        let op = match &data.operator_key {
                            Some(key) => field.find_operator(key),
                            None => field.default_op(),
                        };
                        if let (Some(value), Some(op)) = (data.filter_value.clone(), op) {
                            let mut next = filters.clone();
                            next.push(PowerSearchFilter::new(
                                field.key.clone(),
                                op.key.clone(),
                                value,
                            ));
                            on_change(next, PowerSearchChangeType::Add, filters.len(), window, cx);
                            return;
                        }
                        if let Some(op) = op.filter(|o| matches!(o.value, OperatorValue::Empty)) {
                            let mut next = filters.clone();
                            next.push(PowerSearchFilter::new(
                                field.key.clone(),
                                op.key.clone(),
                                FilterValue::Empty,
                            ));
                            on_change(next, PowerSearchChangeType::Add, filters.len(), window, cx);
                            return;
                        }
                        // FR29: a "More" row carries its typed text in.
                        let opening = data.more_query.clone();
                        opening_state.update(cx, |s, _| s.opening = opening);
                        set_editing(
                            Editing::Adding(PartialFilter {
                                field: field.key.clone(),
                                operator: op.map(|o| o.key.clone()),
                                value: None,
                                read_only: false,
                            }),
                            window,
                            cx,
                        );
                    }
                    TokenizerChangeKind::Remove => {
                        let kept_indices: Vec<usize> =
                            kept.iter().filter_map(|i| i.data.filter_index).collect();
                        if let Some(first) = (0..filters.len()).find(|i| !kept_indices.contains(i))
                        {
                            let next = filters
                                .iter()
                                .enumerate()
                                .filter(|(i, _)| kept_indices.contains(i))
                                .map(|(_, f)| f.clone())
                                .collect();
                            on_change(next, PowerSearchChangeType::Remove, first, window, cx);
                        }
                    }
                    TokenizerChangeKind::Reorder => {}
                }
            }
        };

        // A token's press opens its editor (or commits an empty value).
        let open_token: IndexFn = {
            let (config, filters, on_change, set_editing, components) = (
                config.clone(),
                filters.clone(),
                on_change.clone(),
                set_editing.clone(),
                components.clone(),
            );
            Rc::new(move |index: usize, window: &mut Window, cx: &mut App| {
                if read_only || disabled {
                    return;
                }
                let Some(filter) = filters.get(index) else {
                    return;
                };
                if filter.read_only {
                    return;
                }
                let op = config.find_operator(&filter.field, &filter.operator);
                if commits_without_editor(op, &components) {
                    let mut next = filters.clone();
                    next[index] = PowerSearchFilter::new(
                        filter.field.clone(),
                        filter.operator.clone(),
                        FilterValue::Empty,
                    );
                    on_change(next, PowerSearchChangeType::Edit, index, window, cx);
                    set_editing(Editing::Idle, window, cx);
                    return;
                }
                set_editing(
                    Editing::Edit(
                        index,
                        PartialFilter {
                            field: filter.field.clone(),
                            operator: Some(filter.operator.clone()),
                            value: Some(filter.value.clone()),
                            read_only: false,
                        },
                    ),
                    window,
                    cx,
                );
            })
        };

        let editing = state.read(cx).editing.clone();
        let size = self.size;
        let token_zone = timezone_id.clone();
        let render_token = {
            let (config, filters, open_token, components, state, token_text) = (
                config.clone(),
                filters.clone(),
                open_token.clone(),
                components.clone(),
                state.clone(),
                token_text.clone(),
            );
            let editing = editing.clone();
            let id = self.id.clone();
            move |item: &PowerSearchItem,
                  remove: ActFn,
                  window: &mut Window,
                  cx: &mut App|
                  -> AnyElement {
                let index = item.data.filter_index.unwrap_or(usize::MAX);
                let filter = filters.get(index).cloned();
                let field = config.find_field(&item.data.field_key).cloned();
                let op = item
                    .data
                    .operator_key
                    .as_ref()
                    .and_then(|k| config.find_operator(&item.data.field_key, k))
                    .cloned();
                let can_interact =
                    !read_only && !disabled && !filter.as_ref().is_some_and(|f| f.read_only);
                let click: Option<ActFn> = can_interact.then(|| {
                    let open_token = open_token.clone();
                    Rc::new(move |window: &mut Window, cx: &mut App| open_token(index, window, cx))
                        as ActFn
                });
                let remove = can_interact.then(|| remove.clone());
                // A value type's own token.
                if let (Some(token), Some(filter), Some(field), Some(op)) = (
                    op.as_ref()
                        .and_then(|o| components.get(o.value.kind()))
                        .and_then(|c| c.token.clone()),
                    filter.clone(),
                    field.clone(),
                    op.clone(),
                ) {
                    return token(
                        PowerSearchTokenProps {
                            config: config.clone(),
                            filter,
                            field,
                            operator: op,
                            max_length: max_token_length,
                            on_click: click,
                            on_remove: remove,
                            disabled,
                            timezone_id: token_zone.clone(),
                        },
                        window,
                        cx,
                    );
                }
                let t18 = i18n(cx);
                let (label, value, full) = match &filter {
                    Some(f) => token_text(f, cx),
                    None => {
                        let l = format_token_label(
                            &field
                                .as_ref()
                                .map_or(String::new(), |f| f.label.to_string()),
                            &op.as_ref()
                                .map(|o| o.label_text(cx).to_string())
                                .unwrap_or_default(),
                            &t18,
                        );
                        (l.clone(), None, l)
                    }
                };
                let parts = value.is_none() || token_text_keeps_parts(&t18);
                let token_label = if parts { label } else { full.clone() };
                let mut token = Token::new(
                    ElementId::Name(format!("{id}-token-{index}").into()),
                    token_label,
                )
                .naming(full.clone(), full);
                if let Some(size) = size {
                    token = token.size(size);
                }
                if parts && let Some(value) = value {
                    token =
                        token.end_content(div().font_weight(gpui::FontWeight::NORMAL).child(value));
                }
                // One entity with a photo shows it.
                if let Some(FilterValue::EntityList(list)) = filter.as_ref().map(|f| &f.value)
                    && list.len() == 1
                    && let Some(photo) = &list[0].photo
                {
                    token = token.icon(
                        Avatar::new(list[0].label.clone())
                            .src(photo.clone())
                            .size(crate::people::AvatarSize::Px(16.)),
                    );
                }
                if let Some(click) = click.clone() {
                    let opens_editor = !commits_without_editor(op.as_ref(), &components);
                    if opens_editor {
                        token =
                            token.popup(editing == Editing::Edit(index, editing_partial(&editing)));
                    }
                    token = token.on_click(move |_, window, cx| click(window, cx));
                }
                if let Some(remove) = remove {
                    token = token.on_remove(move |_, window, cx| remove(window, cx));
                }
                token = token.disabled(disabled);
                // Where the token sits, for the editor's edge (FR4).
                let state = state.clone();
                let token_selector = format!("{id}-token-{index}");
                div()
                    .debug_selector(move || token_selector)
                    .relative()
                    .flex_none()
                    .max_w_full()
                    .child(token)
                    .child(
                        gpui::canvas(
                            move |bounds, _, cx| {
                                state.update(cx, |s, _| {
                                    s.tokens.insert(index, bounds);
                                })
                            },
                            |_, _, _, _| {},
                        )
                        .absolute()
                        .top_0()
                        .left_0()
                        .size_full(),
                    )
                    .into_any_element()
            }
        };

        // The rows: an icon by value type, the field's description.
        let render_item = {
            let config = config.clone();
            move |item: &PowerSearchItem, _: &mut Window, _: &mut App| -> AnyElement {
                let field = config.find_field(&item.data.field_key);
                let mut row = TypeaheadItem::new(item.label.clone());
                if let Some(field) = field {
                    let icon = field.icon.clone().unwrap_or_else(|| {
                        let op = match &item.data.operator_key {
                            Some(k) => field.find_operator(k),
                            None => field.default_op(),
                        };
                        let name = match op.map(|o| o.value.kind()).unwrap_or("") {
                            "date_absolute" | "date_range" | "date_relative" => "calendar",
                            "time" => "clock",
                            "enum" | "enum_list" => "menu",
                            _ => "search",
                        };
                        name.into()
                    });
                    row = row.icon(
                        Icon::new(icon)
                            .size(IconSize::Sm)
                            .color(IconColor::Secondary),
                    );
                    if let Some(d) = &field.description {
                        row = row.description(d.clone());
                    }
                }
                row.into_any_element()
            }
        };

        let end_content = match (count_text.clone(), self.end_content.take()) {
            (None, None) => None,
            (count, content) => Some(
                div()
                    .flex()
                    .items_center()
                    .gap(t.spacing.s2)
                    .children(count.map(|c| {
                        div()
                            .text_size(t.text.supporting.size)
                            .line_height(relative(t.text.supporting.leading))
                            .text_color(t.color.text_secondary)
                            .whitespace_nowrap()
                            .debug_selector({
                                let s = format!("{}-result-count", self.id);
                                move || s
                            })
                            .child(c)
                    }))
                    .children(content),
            ),
        };

        let has_read_only = filters.iter().any(|f| f.read_only);
        let clear_message = if has_read_only {
            t18.t("@astryx.powersearch.editableFiltersRemoved")
        } else {
            t18.t("@astryx.powersearch.allFiltersRemoved")
        };
        let removable_filters = filters.clone();
        let mut tokenizer = Tokenizer::new(
            ElementId::Name(format!("{}-field", self.id).into()),
            label.clone(),
            MainSource {
                config: config.clone(),
                max_results: self.max_search_results,
                max_items: self.max_operator_menu_items.unwrap_or(10),
                inflight: Rc::default(),
            },
            items,
        )
        .source_key((
            config.name.to_string(),
            config
                .fields
                .iter()
                .map(|f| f.key.to_string())
                .collect::<Vec<_>>(),
            self.max_search_results,
        ))
        .label_hidden(self.label_hidden)
        .entries_on_focus(true)
        .debounce(std::time::Duration::ZERO)
        .max_menu_items(MAX_BROWSE_MENU_ITEMS)
        .placeholder(if filters.is_empty() {
            placeholder.clone()
        } else {
            SharedString::default()
        })
        .clear(self.clear && !read_only)
        .disabled(disabled)
        .auto_focus(self.auto_focus)
        .controller(tokenizer_controller.clone())
        .presentation(crate::overlay::PopoverPresentation::Popover)
        .item_removable(move |item: &PowerSearchItem| {
            !read_only
                && item
                    .data
                    .filter_index
                    .is_none_or(|i| !removable_filters.get(i).is_some_and(|f| f.read_only))
        })
        .clear_all_message(clear_message)
        .render_token(render_token)
        .render_item(render_item)
        .on_change(handle_change)
        .on_input_bounds({
            let state = state.clone();
            move |bounds, cx| state.update(cx, |s, _| s.input = Some(bounds))
        });
        if let Some(size) = self.size {
            tokenizer = tokenizer.size(size);
        }
        if let Some(w) = self.menu_width {
            tokenizer = tokenizer.menu_width(w);
        }
        if let Some(b) = self.token_overflow {
            tokenizer = tokenizer.token_overflow(b);
        }
        if let Some(icon) = self.start_icon.take() {
            tokenizer = tokenizer.start_icon(icon);
        }
        if let Some(end) = end_content {
            tokenizer = tokenizer.end_content(end);
        }
        if let Some(status) = self.status.clone() {
            tokenizer = tokenizer.status(status);
        }
        if let Some(p) = self.status_placement {
            tokenizer = tokenizer.status_placement(p);
        }
        if let Some(m) = self.disabled_message.clone() {
            tokenizer = tokenizer.disabled_message(m);
        }
        if let Some(f) = self.on_focus.clone() {
            tokenizer = tokenizer.on_focus(move |window, cx| f(window, cx));
        }
        if let Some(f) = self.on_blur.clone() {
            tokenizer = tokenizer.on_blur(move |window, cx| f(window, cx));
        }
        tokenizer.style().refine(&self.style);

        // The editor (FR1-FR9): its width and edge, latched for the open.
        let (outer, input, latched, opens) = {
            let s = state.read(cx);
            (s.outer, s.input, s.latched, s.opens)
        };
        let opening = state.read(cx).opening.clone();
        let viewport = window.viewport_size().width;
        let (width, align) = match outer {
            Some(outer) => {
                let opener = match &editing {
                    Editing::Edit(index, _) => state.read(cx).tokens.get(index).copied(),
                    _ => input,
                };
                let (width, align) = editor_geometry(outer, opener, viewport, rtl);
                (width, latched.unwrap_or(align))
            }
            None => (px(EDITOR_MIN), LayerAlign::Start),
        };
        if editing != Editing::Idle && latched.is_none() && outer.is_some() {
            state.update(cx, |s, _| s.latched = Some(align));
        }
        let content = {
            let editing = editing.clone();
            let (config, filters, on_change, set_editing, components) = (
                config.clone(),
                filters.clone(),
                on_change.clone(),
                set_editing.clone(),
                components.clone(),
            );
            let max_items = self.max_operator_menu_items;
            let save_label = save_label.clone();
            let id = self.id.clone();
            move |window: &mut Window, cx: &mut App| -> AnyElement {
                let (mode, partial, index) = match &editing {
                    Editing::Idle => return div().into_any_element(),
                    Editing::Adding(p) => (EditorMode::Create, p.clone(), None),
                    Editing::Edit(i, p) => (EditorMode::Edit, p.clone(), Some(*i)),
                };
                let on_save: SaveFn = {
                    let (filters, on_change, set_editing) =
                        (filters.clone(), on_change.clone(), set_editing.clone());
                    Rc::new(move |saved, window, cx| {
                        match (index, saved) {
                            (None, Some(f)) => {
                                let mut next = filters.clone();
                                next.push(f);
                                on_change(
                                    next,
                                    PowerSearchChangeType::Add,
                                    filters.len(),
                                    window,
                                    cx,
                                );
                            }
                            (Some(i), Some(f)) => {
                                let mut next = filters.clone();
                                next[i] = f;
                                on_change(next, PowerSearchChangeType::Edit, i, window, cx);
                            }
                            (Some(i), None) => {
                                let next = filters
                                    .iter()
                                    .enumerate()
                                    .filter(|(j, _)| *j != i)
                                    .map(|(_, f)| f.clone())
                                    .collect();
                                on_change(next, PowerSearchChangeType::Remove, i, window, cx);
                            }
                            (None, None) => {}
                        }
                        set_editing(Editing::Idle, window, cx);
                    })
                };
                let on_cancel: ActFn = {
                    let set_editing = set_editing.clone();
                    Rc::new(move |window, cx| set_editing(Editing::Idle, window, cx))
                };
                let op_kind = partial
                    .operator
                    .as_ref()
                    .and_then(|o| config.find_operator(&partial.field, o))
                    .map(|o| o.value.kind());
                if let Some(editor) = op_kind
                    .and_then(|k| components.get(k))
                    .and_then(|c| c.editor.clone())
                {
                    return editor(
                        PowerSearchEditorProps {
                            config: config.clone(),
                            filter: partial,
                            mode,
                            on_save,
                            on_cancel,
                            save_label: save_label.clone(),
                            read_only,
                            timezone_id: timezone_id.clone(),
                        },
                        window,
                        cx,
                    );
                }
                let (save, cancel) = (on_save.clone(), on_cancel.clone());
                let mut editor = PowerSearchFilterEditor::new(
                    ElementId::Name(format!("{id}-editor-{opens}").into()),
                    config.clone(),
                    partial,
                    mode,
                    move |f, window, cx| save(f, window, cx),
                    move |window, cx| cancel(window, cx),
                )
                .save_label(save_label.clone())
                .read_only(read_only);
                if let Some(zone) = timezone_id.clone() {
                    editor = editor.timezone_id(zone);
                }
                if let Some(q) = opening.clone() {
                    editor = editor.opening_query(q);
                }
                if let Some(n) = max_items {
                    editor = editor.max_menu_items(n);
                }
                editor.into_any_element()
            }
        };
        let editing_built_in = match &editing {
            Editing::Edit(i, _) => filters
                .get(*i)
                .and_then(|f| config.find_operator(&f.field, &f.operator))
                .is_some_and(|o| {
                    components
                        .get(o.value.kind())
                        .is_none_or(|c| c.token.is_none())
                }),
            _ => false,
        };
        let dialog_label = match &editing {
            Editing::Edit(i, _) if editing_built_in => filters
                .get(*i)
                .map(|f| token_text(f, cx).2)
                .unwrap_or_default(),
            _ => String::new(),
        };
        let field_box = div().relative().w_full().child(tokenizer).child({
            let state = state.clone();
            gpui::canvas(
                move |bounds, _, cx| state.update(cx, |s, _| s.outer = Some(bounds)),
                |_, _, _, _| {},
            )
            .absolute()
            .top_0()
            .left_0()
            .size_full()
        });
        let set_idle = set_editing.clone();
        Popover::new(popover_id, dialog_label)
            .controller(popover)
            .enabled(false)
            .close_button(false)
            .auto_focus(false)
            .modal(false)
            .role(if editing_built_in {
                PopoverRole::Dialog
            } else {
                PopoverRole::None
            })
            .placement(astryx_primitives::Placement::Below)
            .alignment(align)
            .width(Length::Definite(width.into()))
            .padding(SpacingStep::S0)
            .on_open_change(move |open, window, cx| {
                if !open {
                    set_idle(Editing::Idle, window, cx);
                }
            })
            .content_fn(content)
            .child(field_box)
            .into_any_element()
    }
}

fn editing_partial(editing: &Editing) -> PartialFilter {
    match editing {
        Editing::Edit(_, p) | Editing::Adding(p) => p.clone(),
        Editing::Idle => PartialFilter {
            field: SharedString::default(),
            operator: None,
            value: None,
            read_only: false,
        },
    }
}
