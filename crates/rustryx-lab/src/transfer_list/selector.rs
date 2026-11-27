//! Upstream lab `TransferList/TransferListSelector.tsx`: a transfer-list field that opens
//! in a selector popover. The field label, description and trigger stay outside the
//! popup; changes commit immediately by default, and staged behavior keeps a draft
//! behind Apply and Cancel.

use std::cell::RefCell;
use std::rc::Rc;

use gpui::prelude::*;
use gpui::{AnyElement, App, ElementId, Length, SharedString, StyleRefinement, Window, div, px};
use rustryx_components::button::{Button, ButtonVariant};
use rustryx_components::complex_selector::{ComplexCommit, ComplexSelector, ComplexSelectorState};
use rustryx_components::field::{InputStatus, StatusPlacement};
use rustryx_components::layout::{SpacingStep, Stack, StackAlign};
use rustryx_components::reorderable::{ItemKey, Reorderable, ReorderableOptions};
use rustryx_i18n::Arg;
use rustryx_primitives::{ElementSize, Placement};

use super::list::{TransferList, TransferListOption};

/// Upstream `TransferListSelectorCommitBehavior`: when changes commit.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CommitBehavior {
    /// Each list edit commits (the default), with no footer.
    #[default]
    Immediate,
    /// Edits wait in a draft for Apply; Cancel, Escape and light dismiss drop it.
    Staged,
}

/// Upstream `moveValue`: `value` with `key` moved to index `to`.
pub fn move_value(value: &[SharedString], key: &str, to: usize) -> Vec<SharedString> {
    let mut next: Vec<SharedString> = value
        .iter()
        .filter(|v| v.as_ref() != key)
        .cloned()
        .collect();
    // `splice` clamps an index past the end to the end.
    next.insert(to.min(next.len()), key.to_string().into());
    next
}

type ChangeFn = Rc<dyn Fn(Vec<SharedString>, &mut Window, &mut App)>;
type ActionFn = Rc<dyn Fn(Vec<SharedString>, &mut Window, &mut App) -> gpui::Task<()>>;
type RenderOptionFn = Rc<dyn Fn(&TransferListOption, &mut Window, &mut App) -> AnyElement>;

/// The staged draft and the applied value and behavior it was taken from.
struct Draft {
    draft: Vec<SharedString>,
    source: (Vec<SharedString>, CommitBehavior),
}

/// A transfer-list field that opens in a selector popover (upstream lab
/// `TransferListSelector`).
#[derive(IntoElement)]
pub struct TransferListSelector {
    id: ElementId,
    label: SharedString,
    options: Rc<[TransferListOption]>,
    value: Vec<SharedString>,
    on_change: Option<ChangeFn>,
    change_action: Option<ActionFn>,
    commit_behavior: CommitBehavior,
    trigger_label: Option<AnyElement>,
    description: Option<SharedString>,
    label_hidden: bool,
    selected_label: Option<SharedString>,
    available_label: Option<SharedString>,
    search: bool,
    search_label: Option<SharedString>,
    search_placeholder: Option<SharedString>,
    reorderable: Option<Reorderable>,
    select_all: bool,
    clear: bool,
    render_option: Option<RenderOptionFn>,
    selected_empty_text: Option<SharedString>,
    available_empty_text: Option<SharedString>,
    no_results_text: Option<SharedString>,
    apply_label: Option<SharedString>,
    cancel_label: Option<SharedString>,
    size: Option<ElementSize>,
    width: Option<Length>,
    placement: Option<Placement>,
    optional: bool,
    required: bool,
    disabled: bool,
    loading: bool,
    status: Option<InputStatus>,
    status_placement: Option<StatusPlacement>,
    label_tooltip: Option<SharedString>,
    control_style: StyleRefinement,
}

macro_rules! text_setters {
    ($($(#[$doc:meta])* $name:ident),* $(,)?) => {
        $(
            $(#[$doc])*
            pub fn $name(mut self, text: impl Into<SharedString>) -> Self {
                self.$name = Some(text.into());
                self
            }
        )*
    };
}

macro_rules! flag_setters {
    ($($(#[$doc:meta])* $name:ident),* $(,)?) => {
        $(
            $(#[$doc])*
            pub fn $name(mut self, on: bool) -> Self {
                self.$name = on;
                self
            }
        )*
    };
}

impl TransferListSelector {
    /// Upstream `label` (shown outside the popover unless hidden), `options` and the
    /// applied `value`, in display order.
    pub fn new(
        id: impl Into<ElementId>,
        label: impl Into<SharedString>,
        options: impl Into<Rc<[TransferListOption]>>,
        value: impl IntoIterator<Item = impl Into<SharedString>>,
    ) -> Self {
        TransferListSelector {
            id: id.into(),
            label: label.into(),
            options: options.into(),
            value: value.into_iter().map(Into::into).collect(),
            on_change: None,
            change_action: None,
            commit_behavior: CommitBehavior::Immediate,
            trigger_label: None,
            description: None,
            label_hidden: false,
            selected_label: None,
            available_label: None,
            search: false,
            search_label: None,
            search_placeholder: None,
            reorderable: None,
            select_all: false,
            clear: false,
            render_option: None,
            selected_empty_text: None,
            available_empty_text: None,
            no_results_text: None,
            apply_label: None,
            cancel_label: None,
            size: None,
            width: None,
            placement: None,
            optional: false,
            required: false,
            disabled: false,
            loading: false,
            status: None,
            status_placement: None,
            label_tooltip: None,
            control_style: StyleRefinement::default(),
        }
    }

    /// Upstream `onChange`: every permitted edit in immediate behavior, or once when
    /// Apply commits a changed draft.
    pub fn on_change(
        mut self,
        f: impl Fn(Vec<SharedString>, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_change = Some(Rc::new(f));
        self
    }

    /// Upstream `changeAction`: async work after a changed value is applied.
    pub fn change_action(
        mut self,
        f: impl Fn(Vec<SharedString>, &mut Window, &mut App) -> gpui::Task<()> + 'static,
    ) -> Self {
        self.change_action = Some(Rc::new(f));
        self
    }

    /// Upstream `commitBehavior` (immediate by default).
    pub fn commit_behavior(mut self, behavior: CommitBehavior) -> Self {
        self.commit_behavior = behavior;
        self
    }

    /// Upstream `triggerLabel`: the closed trigger's content (`{count} selected`).
    pub fn trigger_label(mut self, label: impl IntoElement) -> Self {
        self.trigger_label = Some(label.into_any_element());
        self
    }

    text_setters! {
        /// Upstream `description`: guidance shown with the external label.
        description,
        /// Upstream `selectedLabel`.
        selected_label,
        /// Upstream `availableLabel`.
        available_label,
        /// Upstream `searchLabel`.
        search_label,
        /// Upstream `searchPlaceholder`.
        search_placeholder,
        /// Upstream `selectedEmptyText`.
        selected_empty_text,
        /// Upstream `availableEmptyText`.
        available_empty_text,
        /// Upstream `noResultsText`.
        no_results_text,
        /// Upstream `applyLabel`: the staged commit action ("Apply").
        apply_label,
        /// Upstream `cancelLabel`: the staged discard action ("Cancel").
        cancel_label,
        /// Upstream `labelTooltip`: help in an InfoTip after the label.
        label_tooltip,
    }

    flag_setters! {
        /// Upstream `isLabelHidden`.
        label_hidden,
        /// Upstream `hasSearch`.
        search,
        /// Upstream `hasSelectAll`.
        select_all,
        /// Upstream `hasClear`.
        clear,
        /// Upstream `isOptional`.
        optional,
        /// Upstream `isRequired`.
        required,
        /// Upstream `isDisabled`.
        disabled,
        /// Upstream `isLoading`.
        loading,
    }

    /// Upstream `reorderable`: an immediate selector hands each move to the object's
    /// `on_reorder`; a staged one moves its draft instead, so Cancel drops the move
    /// with the draft and Apply commits it through `on_change`.
    pub fn reorderable(mut self, reorderable: Reorderable) -> Self {
        self.reorderable = Some(reorderable);
        self
    }

    /// Upstream `renderOption`.
    pub fn render_option(
        mut self,
        f: impl Fn(&TransferListOption, &mut Window, &mut App) -> AnyElement + 'static,
    ) -> Self {
        self.render_option = Some(Rc::new(f));
        self
    }

    /// Upstream `size`.
    pub fn size(mut self, size: ElementSize) -> Self {
        self.size = Some(size);
        self
    }

    /// Upstream `width`: the field and its popup (`min(41rem, 100vw − 32px)` by
    /// default).
    pub fn width(mut self, width: impl Into<Length>) -> Self {
        self.width = Some(width.into());
        self
    }

    /// Upstream `placement`.
    pub fn placement(mut self, placement: Placement) -> Self {
        self.placement = Some(placement);
        self
    }

    /// Upstream `status`.
    pub fn status(mut self, status: InputStatus) -> Self {
        self.status = Some(status);
        self
    }

    /// Upstream `statusVariant`.
    pub fn status_placement(mut self, placement: StatusPlacement) -> Self {
        self.status_placement = Some(placement);
        self
    }

    /// Upstream `controlXstyle`, `controlClassName` and `controlStyle`: the trigger's
    /// style.
    pub fn control_style(mut self, style: StyleRefinement) -> Self {
        self.control_style = style;
        self
    }
}

impl RenderOnce for TransferListSelector {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = rustryx_theme::current(window, cx);
        let t = theme.tokens();
        let i18n = rustryx_i18n::current(cx);
        let apply_label = self
            .apply_label
            .clone()
            .unwrap_or_else(|| i18n.t("@astryx.transferListSelector.apply"));
        let cancel_label = self
            .cancel_label
            .clone()
            .unwrap_or_else(|| i18n.t("@astryx.transferListSelector.cancel"));
        let behavior = self.commit_behavior;
        let value = self.value.clone();
        let state = window.use_keyed_state(
            ElementId::Name(format!("{:?}-transfer-selector", self.id).into()),
            cx,
            |_, _| {
                Rc::new(RefCell::new(Draft {
                    draft: Vec::new(),
                    source: (Vec::new(), CommitBehavior::Immediate),
                }))
            },
        );
        let draft = state.read(cx).clone();
        // A changed applied value or commit behavior discards the staged draft, as the
        // render adjusts it, so no frame paints the discarded draft.
        {
            let mut d = draft.borrow_mut();
            if d.source.0 != value || d.source.1 != behavior {
                d.source = (value.clone(), behavior);
                d.draft = value.clone();
            }
        }
        let width: Length = self.width.unwrap_or_else(|| {
            let viewport = window.viewport_size().width;
            let rem = window.rem_size();
            px(f32::from(rem * 41.).min(f32::from(viewport) - 32.)).into()
        });
        let max_height = window.viewport_size().height - px(32.);
        let trigger = self.trigger_label.unwrap_or_else(|| {
            i18n.format(
                "@astryx.transferListSelector.triggerLabel",
                &[("count", Arg::Num(value.len() as f64))],
            )
            .into_any_element()
        });
        let id = self.id.clone();
        let label = self.label.clone();
        let options = self.options.clone();
        let disabled = self.disabled;
        let owner = self.reorderable.clone();
        let content_props = Rc::new((
            self.selected_label.clone(),
            self.available_label.clone(),
            self.search,
            self.search_label.clone(),
            self.search_placeholder.clone(),
            self.select_all,
            self.clear,
            self.render_option.clone(),
            self.selected_empty_text.clone(),
            self.available_empty_text.clone(),
            self.no_results_text.clone(),
        ));
        let content_draft = draft.clone();
        let applied_value = value.clone();
        let surface_bg = t.color.background_surface;
        let border = t.color.border;
        let border_width = t.border.width;

        let mut selector = ComplexSelector::new(id.clone(), label.clone(), value.clone())
            .trigger_label(trigger)
            .label_hidden(self.label_hidden)
            .optional(self.optional)
            .required(self.required)
            .disabled(disabled)
            .loading(self.loading)
            .width(width)
            .control_style(self.control_style)
            .content_style({
                let mut style = StyleRefinement::default();
                style.size.width = Some(width);
                style.max_size.width = Some(width);
                style.max_size.height = Some(max_height.into());
                style.padding = Default::default();
                style.overflow.x = Some(gpui::Overflow::Hidden);
                style.overflow.y = Some(gpui::Overflow::Hidden);
                style
            })
            .on_open_change({
                let draft = draft.clone();
                let value = value.clone();
                move |open, _, _| {
                    // A staged draft resets after the selector closes.
                    if !open && behavior == CommitBehavior::Staged {
                        draft.borrow_mut().draft = value.clone();
                    }
                }
            })
            .content(
                move |applied: &Vec<SharedString>,
                      commit: ComplexCommit<Vec<SharedString>>,
                      close,
                      state: ComplexSelectorState,
                      _window,
                      _cx| {
                    let staged = behavior == CommitBehavior::Staged;
                    let list_value = if staged {
                        content_draft.borrow().draft.clone()
                    } else {
                        applied.clone()
                    };
                    let busy = state.busy;
                    // The owner's reorder: aimed at the draft when staged, handed to the
                    // owner as it is when immediate.
                    let reorderable = owner.as_ref().map(|owner| {
                        // Adapted, so the owner's `start(key)` finds the list either way.
                        let keep = owner.clone();
                        let options = if staged {
                            let draft = content_draft.clone();
                            ReorderableOptions::new(move |m, window, _| {
                                if busy {
                                    return;
                                }
                                let ItemKey::Text(key) = &m.key else {
                                    return;
                                };
                                let next = move_value(&draft.borrow().draft, key, m.to);
                                draft.borrow_mut().draft = next;
                                window.refresh();
                            })
                        } else {
                            let target = owner.clone();
                            ReorderableOptions::new(move |m, window, cx| {
                                // One pending change at a time: a move while busy is dropped.
                                if busy {
                                    return;
                                }
                                target.on_reorder(m, window, cx);
                            })
                        };
                        owner.adapt(options.is_reorderable(move |key| keep.can_move(key)))
                    });
                    let (
                        selected_label,
                        available_label,
                        search,
                        search_label,
                        search_placeholder,
                        select_all,
                        clear,
                        render_option,
                        selected_empty_text,
                        available_empty_text,
                        no_results_text,
                    ) = (*content_props).clone();
                    let draft = content_draft.clone();
                    let commit_list = commit.clone();
                    let mut list = TransferList::new(
                        ElementId::Name(format!("{id}-list").into()),
                        label.clone(),
                        options.clone(),
                        list_value,
                    )
                    .label_hidden(true)
                    .disabled(disabled)
                    .search(search)
                    .select_all(select_all)
                    .clear(clear)
                    .on_change(move |next, window, cx| {
                        // While a change is in flight the controls stay live, but transfers
                        // wait for it to settle, so one pending action cannot be stacked.
                        if disabled || busy {
                            return;
                        }
                        if staged {
                            draft.borrow_mut().draft = next;
                            window.refresh();
                        } else {
                            commit_list(next, window, cx);
                        }
                    });
                    macro_rules! pass {
                    ($($field:ident => $method:ident),*) => {
                        $(if let Some(v) = $field.clone() { list = list.$method(v); })*
                    };
                }
                    pass!(
                        selected_label => selected_label,
                        available_label => available_label,
                        search_label => search_label,
                        search_placeholder => search_placeholder,
                        selected_empty_text => selected_empty_text,
                        available_empty_text => available_empty_text,
                        no_results_text => no_results_text
                    );
                    if let Some(f) = render_option.clone() {
                        list = list.render_option(move |o, w, cx| f(o, w, cx));
                    }
                    if let Some(r) = reorderable {
                        list = list.reorderable(r);
                    }
                    let fieldset = div()
                        .id(ElementId::Name(format!("{id:?}-fieldset").into()))
                        .flex_grow(1.)
                        .flex_shrink(1.)
                        .min_w_0()
                        .min_h_0()
                        .overflow_y_scroll()
                        .overscroll_y_contain()
                        .child(list);
                    let footer = staged.then(|| {
                        let cancel_draft = content_draft.clone();
                        let reset_to = applied_value.clone();
                        let cancel_close = close.clone();
                        let apply_draft = content_draft.clone();
                        let applied_now = applied_value.clone();
                        let apply_commit = commit.clone();
                        let apply_close = close.clone();
                        Stack::horizontal()
                            .gap(SpacingStep::S2)
                            .h_align(StackAlign::End)
                            .padding(SpacingStep::S3)
                            .flex_shrink_0()
                            .border_t(border_width)
                            .border_color(border)
                            .bg(surface_bg)
                            .child(
                                div()
                                    .debug_selector({
                                        let s = format!("{id}-cancel");
                                        move || s
                                    })
                                    .child(
                                        Button::new(
                                            ElementId::Name(format!("{id:?}-cancel").into()),
                                            cancel_label.clone(),
                                        )
                                        .variant(ButtonVariant::Ghost)
                                        .size(ElementSize::Sm)
                                        .on_click(
                                            move |_, window, cx| {
                                                cancel_draft.borrow_mut().draft = reset_to.clone();
                                                cancel_close(window, cx);
                                            },
                                        ),
                                    ),
                            )
                            .child(
                                div()
                                    .debug_selector({
                                        let s = format!("{id}-apply");
                                        move || s
                                    })
                                    .child(
                                        Button::new(
                                            ElementId::Name(format!("{id:?}-apply").into()),
                                            apply_label.clone(),
                                        )
                                        .variant(ButtonVariant::Primary)
                                        .size(ElementSize::Sm)
                                        .disabled(disabled || busy)
                                        .loading(busy)
                                        .on_click(
                                            move |_, window, cx| {
                                                let draft = apply_draft.borrow().draft.clone();
                                                if draft != applied_now {
                                                    apply_commit(draft, window, cx);
                                                }
                                                apply_close(window, cx);
                                            },
                                        ),
                                    ),
                            )
                    });
                    div()
                        .flex()
                        .flex_col()
                        .min_w_0()
                        .min_h_0()
                        .max_h(max_height)
                        .child(fieldset)
                        .children(footer)
                        .into_any_element()
                },
            );
        if let Some(d) = self.description {
            selector = selector.description(d);
        }
        if let Some(tip) = self.label_tooltip {
            selector = selector.label_tooltip(tip);
        }
        if let Some(size) = self.size {
            selector = selector.size(size);
        }
        if let Some(placement) = self.placement {
            selector = selector.placement(placement);
        }
        if let Some(status) = self.status {
            selector = selector.status(status);
        }
        if let Some(placement) = self.status_placement {
            selector = selector.status_placement(placement);
        }
        if let Some(f) = self.on_change {
            selector = selector.on_change(move |v, w, cx| f(v, w, cx));
        }
        if let Some(f) = self.change_action {
            selector = selector.change_action(move |v, w, cx| f(v, w, cx));
        }
        selector
    }
}
