//! Column settings (upstream `useTableColumnSettings` and
//! `useTableColumnSettingsState`): which columns show, in what order. Headless:
//! pair it with any column picker.

use std::collections::HashSet;
use std::rc::Rc;

use gpui::{App, SharedString, Window};

use super::model::TableColumn;
use super::plugin::TablePlugin;
use crate::doc::{ComponentDoc, PropMapping, RustApi};

/// A column the settings offer (upstream `ColumnSettingsOption`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ColumnSettingsOption {
    /// The column's key.
    pub key: SharedString,
    /// Its name in the picker.
    pub label: SharedString,
    /// Upstream `isAlwaysVisible`: it can never be hidden.
    pub always_visible: bool,
    /// Upstream `group`: the picker lists a group's columns together.
    pub group: Option<SharedString>,
}

impl ColumnSettingsOption {
    pub fn new(key: impl Into<SharedString>, label: impl Into<SharedString>) -> Self {
        Self {
            key: key.into(),
            label: label.into(),
            always_visible: false,
            group: None,
        }
    }

    /// Upstream `isAlwaysVisible`.
    pub fn always_visible(mut self, always: bool) -> Self {
        self.always_visible = always;
        self
    }

    /// Upstream `group`.
    pub fn group(mut self, group: impl Into<SharedString>) -> Self {
        self.group = Some(group.into());
        self
    }
}

type KeysFn = Rc<dyn Fn(Vec<SharedString>, &mut Window, &mut App)>;

/// The settings (upstream `UseTableColumnSettingsStateConfig`, also the
/// plugin's config).
#[derive(Clone)]
pub struct ColumnSettingsConfig {
    pub columns: Rc<[ColumnSettingsOption]>,
    /// The shown columns, in order.
    pub active_column_keys: Vec<SharedString>,
    on_change: KeysFn,
    pub default_column_keys: Option<Vec<SharedString>>,
}

impl ColumnSettingsConfig {
    /// Upstream `columns`, `activeColumnKeys` and `onChangeActiveColumnKeys`.
    pub fn new(
        columns: impl IntoIterator<Item = ColumnSettingsOption>,
        active_column_keys: impl IntoIterator<Item = impl Into<SharedString>>,
        on_change_active_column_keys: impl Fn(Vec<SharedString>, &mut Window, &mut App) + 'static,
    ) -> Self {
        Self {
            columns: columns.into_iter().collect(),
            active_column_keys: active_column_keys.into_iter().map(Into::into).collect(),
            on_change: Rc::new(on_change_active_column_keys),
            default_column_keys: None,
        }
    }

    /// Upstream `defaultColumnKeys`: what a reset restores (every column
    /// without it).
    pub fn default_column_keys(
        mut self,
        keys: impl IntoIterator<Item = impl Into<SharedString>>,
    ) -> Self {
        self.default_column_keys = Some(keys.into_iter().map(Into::into).collect());
        self
    }

    fn always_visible(&self) -> impl Iterator<Item = &SharedString> {
        self.columns
            .iter()
            .filter(|c| c.always_visible)
            .map(|c| &c.key)
    }
}

/// The plugin (upstream `useTableColumnSettings`): only the active columns,
/// in their order. It runs first among the known plugins, before others add
/// their own columns.
#[derive(Clone)]
pub struct TableColumnSettings {
    active: Rc<[SharedString]>,
}

/// Upstream `useTableColumnSettings`.
pub fn use_table_column_settings(config: &ColumnSettingsConfig) -> TableColumnSettings {
    TableColumnSettings {
        active: config.active_column_keys.iter().cloned().collect(),
    }
}

impl<T> TablePlugin<T> for TableColumnSettings {
    fn transform_columns(
        &self,
        columns: Vec<TableColumn<T>>,
        _: &mut Window,
        _: &mut App,
    ) -> Vec<TableColumn<T>> {
        let position = |key: &SharedString| self.active.iter().position(|k| k == key);
        let mut shown: Vec<(usize, TableColumn<T>)> = columns
            .into_iter()
            .filter_map(|col| position(&col.key).map(|i| (i, col)))
            .collect();
        shown.sort_by_key(|(i, _)| *i);
        shown.into_iter().map(|(_, col)| col).collect()
    }
}

/// The state's operations (upstream `UseTableColumnSettingsStateReturn`).
#[derive(Clone)]
pub struct ColumnSettingsState {
    /// Ready for [`use_table_column_settings`] (upstream
    /// `columnSettingsConfig`).
    pub config: ColumnSettingsConfig,
    pub active_column_keys: Vec<SharedString>,
}

impl ColumnSettingsState {
    /// Upstream `toggleColumn`: hides an active column, or adds it at the
    /// end; never an always-visible one.
    pub fn toggle_column(&self, key: &str, window: &mut Window, cx: &mut App) {
        if self.config.always_visible().any(|k| k.as_ref() == key) {
            return;
        }
        let keys = &self.config.active_column_keys;
        let next: Vec<SharedString> = if keys.iter().any(|k| k.as_ref() == key) {
            keys.iter().filter(|k| k.as_ref() != key).cloned().collect()
        } else {
            keys.iter()
                .cloned()
                .chain([SharedString::from(key.to_string())])
                .collect()
        };
        (self.config.on_change)(next, window, cx)
    }

    /// Upstream `isColumnActive`.
    pub fn is_column_active(&self, key: &str) -> bool {
        self.active_column_keys.iter().any(|k| k.as_ref() == key)
    }

    /// Upstream `isColumnToggleable`.
    pub fn is_column_toggleable(&self, key: &str) -> bool {
        !self.config.always_visible().any(|k| k.as_ref() == key)
    }

    /// Upstream `showAllColumns`.
    pub fn show_all_columns(&self, window: &mut Window, cx: &mut App) {
        let all = self.config.columns.iter().map(|c| c.key.clone()).collect();
        (self.config.on_change)(all, window, cx)
    }

    /// Upstream `resetToDefault`: the default keys, else every column.
    pub fn reset_to_default(&self, window: &mut Window, cx: &mut App) {
        match &self.config.default_column_keys {
            Some(keys) => (self.config.on_change)(keys.clone(), window, cx),
            None => self.show_all_columns(window, cx),
        }
    }

    /// Upstream `setActiveColumnKeys`: these keys, the always-visible ones
    /// kept.
    pub fn set_active_column_keys(
        &self,
        keys: impl IntoIterator<Item = impl Into<SharedString>>,
        window: &mut Window,
        cx: &mut App,
    ) {
        let mut next: Vec<SharedString> = Vec::new();
        let mut seen: HashSet<SharedString> = HashSet::new();
        for key in keys
            .into_iter()
            .map(Into::into)
            .chain(self.config.always_visible().cloned())
        {
            if seen.insert(key.clone()) {
                next.push(key);
            }
        }
        (self.config.on_change)(next, window, cx)
    }
}

/// Upstream `useTableColumnSettingsState`.
pub fn use_table_column_settings_state(config: ColumnSettingsConfig) -> ColumnSettingsState {
    ColumnSettingsState {
        active_column_keys: config.active_column_keys.clone(),
        config,
    }
}

const fn p(prop: &'static str, rust: RustApi) -> PropMapping {
    PropMapping { prop, rust }
}

pub const USE_TABLE_COLUMN_SETTINGS_DOC: ComponentDoc = ComponentDoc {
    name: "useTableColumnSettings",
    rust_type: "rustryx::components::table::TableColumnSettings",
    props: &[
        p("columns", RustApi::Argument("ColumnSettingsConfig::new")),
        p(
            "activeColumnKeys",
            RustApi::Argument("ColumnSettingsConfig::new"),
        ),
        p(
            "onChangeActiveColumnKeys",
            RustApi::Argument("ColumnSettingsConfig::new"),
        ),
        p("defaultColumnKeys", RustApi::Method("default_column_keys")),
    ],
    example: include_str!("examples.rs"),
};

pub const USE_TABLE_COLUMN_SETTINGS_STATE_DOC: ComponentDoc = ComponentDoc {
    name: "useTableColumnSettingsState",
    rust_type: "rustryx::components::table::use_table_column_settings_state",
    props: &[
        p("config", RustApi::Argument("ColumnSettingsConfig")),
        p(
            "config.columns",
            RustApi::Argument("ColumnSettingsConfig::new"),
        ),
        p(
            "config.activeColumnKeys",
            RustApi::Argument("ColumnSettingsConfig::new"),
        ),
        p(
            "config.onChangeActiveColumnKeys",
            RustApi::Argument("ColumnSettingsConfig::new"),
        ),
        p(
            "config.defaultColumnKeys",
            RustApi::Method("default_column_keys"),
        ),
    ],
    example: include_str!("examples.rs"),
};
