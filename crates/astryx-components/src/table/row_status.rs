//! The row-status column (upstream `useTableRowStatus`): a narrow first
//! column with each row's status as a themed glyph or a coloured dot, named
//! for assistive technology and shown in a tooltip.

use std::rc::Rc;

use astryx_i18n::current as i18n;
use astryx_theme::current;
use gpui::{
    AnyElement, App, ElementId, Hsla, InteractiveElement, IntoElement, ParentElement, Role,
    SharedString, Styled, Window, div, prelude::*, px,
};

use super::model::{TableColumn, TableItem, pixel};
use super::plugin::{HeaderCellProps, TablePlugin};
use crate::doc::{ComponentDoc, PropMapping, RustApi};
use crate::icon::{Icon, IconColor, IconSize, IconSource};
use crate::overlay::Tooltip;

/// The status column's key.
pub const ROW_STATUS_COLUMN_KEY: &str = "__rowStatus";

/// A semantic status: the theme's glyph and tone (upstream
/// `TableSemanticRowStatus['status']`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TableRowSemanticStatus {
    Success,
    Warning,
    Error,
}

/// A custom marker's named colour (upstream `TableRowStatusColor`): the
/// icon colour tokens.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TableRowStatusColor {
    Accent,
    Success,
    Error,
    Warning,
    Red,
    Orange,
    Green,
    Yellow,
    Blue,
    Gray,
}

/// A custom marker's colour: a named one, or any colour (upstream's raw CSS
/// colour).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum StatusColor {
    Named(TableRowStatusColor),
    Raw(Hsla),
}

impl From<TableRowStatusColor> for StatusColor {
    fn from(color: TableRowStatusColor) -> Self {
        StatusColor::Named(color)
    }
}

impl From<Hsla> for StatusColor {
    fn from(color: Hsla) -> Self {
        StatusColor::Raw(color)
    }
}

#[derive(Clone, Debug)]
enum StatusKind {
    Semantic(TableRowSemanticStatus),
    Custom {
        color: StatusColor,
        icon: Option<IconSource>,
    },
}

/// A row's status (upstream `TableRowStatus` or `TableSemanticRowStatus`):
/// a semantic status, or a custom marker, a dot unless it has an icon.
/// `label` names it.
#[derive(Clone, Debug)]
pub struct TableRowStatus {
    kind: StatusKind,
    label: SharedString,
}

impl TableRowStatus {
    /// `{status: 'success', label}`.
    pub fn success(label: impl Into<SharedString>) -> Self {
        Self::semantic(TableRowSemanticStatus::Success, label)
    }

    /// `{status: 'warning', label}`.
    pub fn warning(label: impl Into<SharedString>) -> Self {
        Self::semantic(TableRowSemanticStatus::Warning, label)
    }

    /// `{status: 'error', label}`.
    pub fn error(label: impl Into<SharedString>) -> Self {
        Self::semantic(TableRowSemanticStatus::Error, label)
    }

    /// A semantic status: its themed glyph and tone.
    pub fn semantic(status: TableRowSemanticStatus, label: impl Into<SharedString>) -> Self {
        Self {
            kind: StatusKind::Semantic(status),
            label: label.into(),
        }
    }

    /// `{color, label}`: a dot in `color`.
    pub fn color(color: impl Into<StatusColor>, label: impl Into<SharedString>) -> Self {
        Self {
            kind: StatusKind::Custom {
                color: color.into(),
                icon: None,
            },
            label: label.into(),
        }
    }

    /// `icon` on a custom marker: the icon in its colour instead of a dot.
    /// A semantic status keeps its own glyph (upstream ignores `icon` with
    /// `status`).
    pub fn icon(mut self, icon: impl Into<IconSource>) -> Self {
        match &mut self.kind {
            StatusKind::Custom { icon: slot, .. } => *slot = Some(icon.into()),
            StatusKind::Semantic(_) => {
                super::plugin::dev_warn_once("useTableRowStatus:semantic-custom-conflict", || {
                    "useTableRowStatus: status cannot be combined with color or icon. The \
                     semantic status takes precedence and the custom marker fields are ignored."
                        .into()
                })
            }
        }
        self
    }

    pub fn label(&self) -> &SharedString {
        &self.label
    }
}

type StatusFn<T> = Rc<dyn Fn(&T) -> Option<TableRowStatus>>;

/// The plugin (upstream `useTableRowStatus`): a 28 px first column.
pub struct TableRowStatusPlugin<T> {
    get_status: StatusFn<T>,
}

impl<T: 'static> TableRowStatusPlugin<T> {
    /// Upstream `getStatus`: a row's status, or `None` for no indicator.
    pub fn new(get_status: impl Fn(&T) -> Option<TableRowStatus> + 'static) -> Self {
        Self {
            get_status: Rc::new(get_status),
        }
    }
}

/// Upstream `useTableRowStatus`.
pub fn use_table_row_status<T: 'static>(
    get_status: impl Fn(&T) -> Option<TableRowStatus> + 'static,
) -> TableRowStatusPlugin<T> {
    TableRowStatusPlugin::new(get_status)
}

fn named_color(t: &astryx_theme::Tokens, color: TableRowStatusColor) -> Hsla {
    use TableRowStatusColor::*;
    match color {
        Accent => t.color.icon_accent,
        Success | Green => t.color.icon_green,
        Error | Red => t.color.icon_red,
        Warning | Orange => t.color.icon_orange,
        Yellow => t.color.icon_yellow,
        Blue => t.color.icon_blue,
        Gray => t.color.icon_gray,
    }
}

/// The released Icon colour for a named marker colour.
fn icon_color(color: TableRowStatusColor) -> IconColor {
    use TableRowStatusColor::*;
    match color {
        Accent => IconColor::Accent,
        Success => IconColor::Success,
        Error => IconColor::Error,
        Warning | Orange | Yellow => IconColor::Warning,
        Red => IconColor::Red,
        Green => IconColor::Green,
        Blue => IconColor::Blue,
        Gray => IconColor::Gray,
    }
}

fn status_cell(
    status: &TableRowStatus,
    index: usize,
    window: &mut Window,
    cx: &mut App,
) -> AnyElement {
    let theme = current(window, cx);
    let t = theme.tokens();
    let mut wrap = div()
        .id(ElementId::NamedInteger("row-status".into(), index as u64))
        .flex()
        .items_center()
        .justify_center()
        .role(Role::Image)
        .aria_label(status.label.clone());
    let signifier = match &status.kind {
        StatusKind::Semantic(s) => {
            let (name, color) = match s {
                TableRowSemanticStatus::Success => ("success", IconColor::Success),
                TableRowSemanticStatus::Warning => ("warning", IconColor::Warning),
                TableRowSemanticStatus::Error => ("error", IconColor::Error),
            };
            Icon::new(name)
                .size(IconSize::Xsm)
                .color(color)
                .into_any_element()
        }
        StatusKind::Custom {
            color,
            icon: Some(icon),
        } => {
            let icon = Icon::new(icon.clone()).size(IconSize::Xsm);
            match color {
                StatusColor::Named(c) => icon.color(icon_color(*c)).into_any_element(),
                StatusColor::Raw(c) => {
                    wrap = wrap.text_color(*c);
                    icon.color(IconColor::Inherit).into_any_element()
                }
            }
        }
        StatusKind::Custom { color, icon: None } => {
            let fill = match color {
                StatusColor::Named(c) => named_color(t, *c),
                StatusColor::Raw(c) => *c,
            };
            div()
                .size(px(8.))
                .rounded_full()
                .flex_shrink_0()
                .bg(fill)
                .into_any_element()
        }
    };
    Tooltip::new(
        ElementId::NamedInteger("row-status-tip".into(), index as u64),
        status.label.clone(),
    )
    .child(wrap.child(signifier))
    .into_any_element()
}

impl<T: 'static> TablePlugin<T> for TableRowStatusPlugin<T> {
    fn transform_columns(
        &self,
        mut columns: Vec<TableColumn<T>>,
        _: &mut Window,
        _: &mut App,
    ) -> Vec<TableColumn<T>> {
        let get_status = self.get_status.clone();
        columns.insert(
            0,
            TableColumn::new(ROW_STATUS_COLUMN_KEY)
                // The gutter stays blank; the header is named below.
                .header_element(|_, _| div().into_any_element())
                .width(pixel(28.))
                .resizable(false)
                .render_cell_item(
                    move |item: &TableItem<T>, window, cx| match get_status(item) {
                        Some(status) => status_cell(&status, item.index(), window, cx),
                        None => div().into_any_element(),
                    },
                ),
        );
        columns
    }

    fn transform_header_cell(
        &self,
        mut props: HeaderCellProps,
        column: &TableColumn<T>,
        _: &[TableColumn<T>],
        _: &mut Window,
        cx: &mut App,
    ) -> HeaderCellProps {
        // A blank gutter still needs a name (axe: empty-table-header).
        if column.key.as_ref() == ROW_STATUS_COLUMN_KEY {
            props.aria_label = Some(i18n(cx).t("@astryx.table.rowStatus.columnHeader"));
        }
        props
    }
}

const fn p(prop: &'static str, rust: RustApi) -> PropMapping {
    PropMapping { prop, rust }
}

pub const USE_TABLE_ROW_STATUS_DOC: ComponentDoc = ComponentDoc {
    name: "useTableRowStatus",
    rust_type: "astryx::components::table::use_table_row_status",
    props: &[p(
        "getStatus",
        RustApi::Argument("get_status (TableRowStatus::success / warning / error / color)"),
    )],
    example: include_str!("examples.rs"),
};
