//! Row activation (upstream `useTableActivation`, an accepted contract
//! without an upstream implementation yet): one controlled current row for
//! master-detail layouts. A primary click, or Enter or Space on the focused
//! row, asks for it; the active row shows `aria-current`, a fill and an
//! inline-start marker.
//!
//! Only flat, data-driven product rows with a valid unique `id_key` take
//! part (FR2, FR9, FR11); with another plugin claiming the whole row's
//! press, activation stays inert (FR9). Presses on nested controls never
//! activate (FR6): a control that handles its own click stops it, and one
//! that takes the focus on press holds it.

use std::rc::Rc;

use gpui::{App, Window};

use super::model::{IdentityStatus, RowKey, TableItem};
use super::plugin::{
    BodyRowProps, RowKind, RowModel, TableMode, TablePlugin, TableRowContext, WholeRowOwner,
    dev_warn_once,
};
use crate::doc::{ComponentDoc, PropMapping, RustApi};

type ActivateFn<T> = Rc<dyn Fn(&T, &RowKey, &mut Window, &mut App)>;
type Predicate<T> = Rc<dyn Fn(&T) -> bool>;

/// The plugin (upstream `useTableActivation`).
pub struct TableActivation<T> {
    active_key: Option<RowKey>,
    on_activate: ActivateFn<T>,
    is_activatable: Option<Predicate<T>>,
}

impl<T: 'static> TableActivation<T> {
    /// Upstream `activeKey` (a string or number key, or `None`) and
    /// `onActivate` (the row and its key, exactly as `id_key` gives it).
    pub fn new(
        active_key: Option<impl Into<RowKey>>,
        on_activate: impl Fn(&T, &RowKey, &mut Window, &mut App) + 'static,
    ) -> Self {
        Self {
            active_key: active_key.map(Into::into),
            on_activate: Rc::new(on_activate),
            is_activatable: None,
        }
    }

    /// Upstream `getIsItemActivatable`: rows it rejects neither activate
    /// nor show as active.
    pub fn is_item_activatable(mut self, f: impl Fn(&T) -> bool + 'static) -> Self {
        self.is_activatable = Some(Rc::new(f));
        self
    }

    /// Whether the row takes part, by the table's admission, its identity
    /// and the caller's predicate (FR2, FR4, FR9, FR11).
    fn eligible(&self, item: &T, props: &BodyRowProps, context: &TableRowContext) -> bool {
        let render = &context.render;
        if render.mode != TableMode::Data || context.kind != RowKind::Product {
            return false;
        }
        if render.row_model != RowModel::Flat || render.whole_row_owners > 1 {
            dev_warn_once("useTableActivation:admission", || {
                "useTableActivation: activation needs a flat row model and no other \
                 whole-row owner (row expansion, navigation); it stays inert."
                    .into()
            });
            return false;
        }
        if context.identity_status != IdentityStatus::Valid || context.key.is_none() {
            dev_warn_once("useTableActivation:identity", || {
                format!(
                    "useTableActivation: a row's id_key is {:?}; such rows never activate. \
                     Give every row a unique string or finite number key.",
                    context.identity_status
                )
            });
            return false;
        }
        // Another owner of the row's press or current state wins (FR3).
        if props.row_action.is_some() || props.paint.active {
            dev_warn_once("useTableActivation:owner", || {
                "useTableActivation: the row already has a whole-row action or a \
                 current state; activation leaves it alone."
                    .into()
            });
            return false;
        }
        self.is_activatable.as_ref().is_none_or(|f| f(item))
    }
}

/// Upstream `useTableActivation`.
pub fn use_table_activation<T: 'static>(
    active_key: Option<impl Into<RowKey>>,
    on_activate: impl Fn(&T, &RowKey, &mut Window, &mut App) + 'static,
) -> TableActivation<T> {
    TableActivation::new(active_key, on_activate)
}

impl<T: 'static> TablePlugin<T> for TableActivation<T> {
    fn row_model(&self) -> Option<RowModel> {
        Some(RowModel::Flat)
    }

    fn transforms_body(&self) -> bool {
        true
    }

    fn whole_row_owner(&self) -> Option<WholeRowOwner> {
        Some(WholeRowOwner::Activation)
    }

    fn needs_canonical_identity(&self) -> bool {
        true
    }

    fn transform_body_row(
        &self,
        mut props: BodyRowProps,
        item: &TableItem<T>,
        context: &TableRowContext,
        _: &mut Window,
        _: &mut App,
    ) -> BodyRowProps {
        if !self.eligible(item, &props, context) {
            return props;
        }
        let Some(key) = context.key.clone() else {
            return props;
        };
        // Strict: a number key never matches its digits as a string (FR2).
        props.paint.active = self.active_key.as_ref() == Some(&key);
        // A tab stop unless an earlier plugin set the row's focus (FR7).
        if props.tab_stop.is_none() {
            props.tab_stop = Some(true);
        }
        for token in ["Enter", "Space"] {
            if !props.key_shortcuts.iter().any(|t| t.as_ref() == token) {
                props.key_shortcuts.push(token.into());
            }
        }
        let (on_activate, row) = (self.on_activate.clone(), item.clone());
        props.row_action = Some(Rc::new(move |window, cx| {
            on_activate(&row, &key, window, cx)
        }));
        props
    }
}

const fn p(prop: &'static str, rust: RustApi) -> PropMapping {
    PropMapping { prop, rust }
}

pub const USE_TABLE_ACTIVATION_DOC: ComponentDoc = ComponentDoc {
    name: "useTableActivation",
    rust_type: "astryx::components::table::TableActivation",
    props: &[
        p("activeKey", RustApi::Argument("active_key")),
        p("onActivate", RustApi::Argument("on_activate")),
        p(
            "getIsItemActivatable",
            RustApi::Method("is_item_activatable"),
        ),
    ],
    example: include_str!("examples.rs"),
};
