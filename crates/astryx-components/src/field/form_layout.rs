//! FormLayout. Upstream: `FormLayout/`.
//!
//! `horizontal-labels` is a two-column grid in CSS (`auto 1fr`) whose cells
//! are each Field's label and control (the Field box is `display:
//! contents`). Taffy has no `display: contents`, so here each Field draws
//! its own row: a label cell as wide as the layout's widest label, then the
//! control. The labels report their natural widths each frame; when the
//! widest changes, the layout takes it and draws again.

use astryx_primitives::{provide, use_context};
use astryx_theme::current;
use gpui::{
    AnyElement, App, ElementId, Entity, IntoElement, ParentElement, Pixels, RenderOnce,
    StyleRefinement, Styled, Window, div, prelude::*, px,
};
use smallvec::SmallVec;

/// Upstream `direction`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum FormDirection {
    /// Fields stack top to bottom.
    #[default]
    Vertical,
    /// One equal column per field, in reading order.
    Horizontal,
    /// Labels beside their controls, in one shared label column. At a
    /// window 480px wide or narrower, fields stack again.
    HorizontalLabels,
}

impl FormDirection {
    pub fn as_str(self) -> &'static str {
        match self {
            FormDirection::Vertical => "vertical",
            FormDirection::Horizontal => "horizontal",
            FormDirection::HorizontalLabels => "horizontal-labels",
        }
    }
}

/// Upstream `defaultOptionality`: the state a form treats as normal, so
/// only the exception shows a marker.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Optionality {
    /// Fields are optional unless marked required: no "Optional" markers.
    Optional,
    /// Fields are required unless marked optional: no "Required" markers,
    /// and unmarked fields report required to assistive technology.
    Required,
}

/// Upstream `defaultEntryErrorBehavior`: when date, time and number fields
/// show their own rejection of what was typed.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum EntryErrorBehavior {
    /// When the entry is complete.
    #[default]
    Complete,
    /// Only at blur or Enter.
    BlurOrEnter,
}

/// The widest label of a `horizontal-labels` layout.
#[derive(Default)]
pub(crate) struct LabelColumn {
    /// The width the label cells take this frame.
    pub width: Option<Pixels>,
    /// The widest natural label width seen this frame.
    pub measured: Pixels,
}

/// What a FormLayout tells the fields inside it (upstream
/// `FormLayoutContext`). A nested layout replaces all of it.
#[derive(Clone, Default)]
pub(crate) struct FormContext {
    pub direction: FormDirection,
    pub optionality: Option<Optionality>,
    /// Read by the date and time inputs; NumberInput behaves the same under
    /// both values (its FR14).
    #[allow(dead_code)]
    pub entry_error_behavior: Option<EntryErrorBehavior>,
    /// The shared label column, in a `horizontal-labels` layout that has
    /// not collapsed.
    pub labels: Option<Entity<LabelColumn>>,
    /// A `horizontal-labels` layout collapsed to one column.
    pub collapsed: bool,
}

pub(crate) fn form_context(cx: &App) -> FormContext {
    use_context::<FormContext>(cx).unwrap_or_default()
}

/// A field's required state and markers under its form's default,
/// upstream `useResolvedOptionality`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Resolved {
    /// Reported to assistive technology.
    pub required: bool,
    pub shows_required: bool,
    pub shows_optional: bool,
}

pub(crate) fn resolve(required: bool, optional: bool, cx: &App) -> Resolved {
    resolved(form_context(cx).optionality, required, optional)
}

pub(crate) fn resolved(default: Option<Optionality>, required: bool, optional: bool) -> Resolved {
    let shows_optional = optional && default != Some(Optionality::Optional);
    Resolved {
        required: !optional && (required || default == Some(Optionality::Required)),
        shows_required: !shows_optional && required && default != Some(Optionality::Required),
        shows_optional,
    }
}

/// The window width at or below which `horizontal-labels` stacks.
const COLLAPSE_WIDTH: Pixels = px(480.);

/// Arranges form fields with consistent spacing, upstream `FormLayout`.
#[derive(IntoElement)]
pub struct FormLayout {
    id: ElementId,
    direction: FormDirection,
    optionality: Option<Optionality>,
    entry_error_behavior: Option<EntryErrorBehavior>,
    style: StyleRefinement,
    children: SmallVec<[AnyElement; 4]>,
}

impl FormLayout {
    pub fn new(id: impl Into<ElementId>) -> Self {
        FormLayout {
            id: id.into(),
            direction: FormDirection::Vertical,
            optionality: None,
            entry_error_behavior: None,
            style: StyleRefinement::default(),
            children: SmallVec::new(),
        }
    }

    astryx_primitives::setters! {
        /// Upstream `direction`.
        direction: FormDirection,
        /// Upstream `defaultOptionality`.
        ?optionality: Optionality,
        /// Upstream `defaultEntryErrorBehavior`.
        ?entry_error_behavior: EntryErrorBehavior,
    }
}

impl Styled for FormLayout {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl ParentElement for FormLayout {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(elements);
    }
}

impl RenderOnce for FormLayout {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = current(window, cx);
        let t = theme.tokens();
        let collapsed = self.direction == FormDirection::HorizontalLabels
            && window.viewport_size().width <= COLLAPSE_WIDTH;
        let labels = (self.direction == FormDirection::HorizontalLabels && !collapsed).then(|| {
            let labels = window.use_keyed_state(
                ElementId::Name(format!("{:?}-labels", self.id).into()),
                cx,
                |_, _| LabelColumn::default(),
            );
            labels.update(cx, |l, _| l.measured = px(0.));
            labels
        });

        let count = self.children.len().max(1) as u16;
        let mut el = div();
        el = match self.direction {
            FormDirection::Vertical => el.flex().flex_col().gap(t.spacing.s4),
            FormDirection::Horizontal => el.grid().grid_cols(count).gap(t.spacing.s4),
            FormDirection::HorizontalLabels if collapsed => el.flex().flex_col().gap(t.spacing.s4),
            FormDirection::HorizontalLabels => el.flex().flex_col().gap(t.spacing.s3),
        };
        if let Some(labels) = &labels {
            // Once every label has reported, the widest sets the column.
            let labels = labels.clone();
            el = el.on_children_prepainted(move |_, window, cx| {
                let changed = labels.update(cx, |l, _| {
                    let changed = l.width != Some(l.measured);
                    l.width = Some(l.measured);
                    changed
                });
                if changed {
                    window.defer(cx, |window, _| window.refresh());
                }
            });
        }
        theme.refine(
            el.style(),
            "form-layout",
            &[("direction", self.direction.as_str())],
            None,
        );
        el.style().refine(&self.style);
        let context = FormContext {
            direction: self.direction,
            optionality: self.optionality,
            entry_error_behavior: self.entry_error_behavior,
            labels,
            collapsed,
        };
        provide(context, el.id(self.id).children(self.children))
    }
}
