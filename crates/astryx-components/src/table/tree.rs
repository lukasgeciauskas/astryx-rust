//! Tree data (upstream `useTableTreeData` and `useTableTreeState`): nested
//! rows flattened to the visible ones, with each row's indent and expander
//! on the tree column. The disclosure pattern, not a treegrid: an expander
//! carries `aria-expanded` and, while expanded, `aria-controls` naming its
//! child rows (fork patch R15); rows carry no level.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use astryx_i18n::{LogicalStyled, current as i18n, use_direction};
use astryx_primitives::{control_focus, is_focus_visible, use_context};
use astryx_theme::current;
use gpui::{
    AnyElement, App, Context, ElementId, InteractiveElement, IntoElement, ParentElement,
    RenderOnce, Role, SharedString, StatefulInteractiveElement, Styled, Window, div, px,
};

use super::cells::{CellDensity, TableDensity};
use super::model::{IdKey, TableColumn, TableItem, TableRecord};
use super::plugin::{BodyRowProps, HeaderCellProps, RowModel, TablePlugin, TableRowContext};
use crate::doc::{ComponentDoc, PropMapping, RustApi};
use crate::icon::{Icon, IconSize};

/// One visible row's place in the tree (upstream `TableTreeRowMeta`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TableTreeRowMeta {
    /// Its id, from `id_key`.
    pub id: SharedString,
    /// Its depth: roots are 0.
    pub level: usize,
    /// Whether it shows an expander.
    pub has_children: bool,
    pub is_expanded: bool,
}

/// The indent per level (upstream `indent`): spacing steps 3, 4 or 6.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TreeIndent {
    Sm,
    #[default]
    Md,
    Lg,
}

/// How many expandable rows are expanded (upstream `isAllExpanded`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AllExpanded {
    None,
    All,
    /// Some, not all.
    Some,
}

type MetaFn<T> = Rc<dyn Fn(&T) -> Option<TableTreeRowMeta>>;
type ItemFn<T> = Rc<dyn Fn(&T, &mut Window, &mut App)>;
type LabelFn<T> = Rc<dyn Fn(&T) -> SharedString>;
type ActionFn = Rc<dyn Fn(&mut Window, &mut App)>;
type ChildrenFn<T> = Rc<dyn for<'a> Fn(&'a T) -> &'a [T]>;
type IdsFn = Rc<dyn Fn(HashSet<SharedString>, &mut Window, &mut App)>;
type Predicate<T> = Rc<dyn Fn(&T) -> bool>;
type SortFn<T> = Rc<dyn Fn(Vec<T>) -> Vec<T>>;

/// The visible rows, in order, as the body transform sees them (id, level,
/// accessibility name): what an expander's `aria-controls` reads.
type RowLevels = Rc<RefCell<Vec<(SharedString, usize, SharedString)>>>;

/// The plugin (upstream `useTableTreeData`).
pub struct TableTreeData<T> {
    get_row_meta: MetaFn<T>,
    on_toggle_item: ItemFn<T>,
    row_label: Option<LabelFn<T>>,
    has_expandable_rows: bool,
    expand_all_control: bool,
    all_expanded: Option<AllExpanded>,
    on_expand_all: Option<ActionFn>,
    on_collapse_all: Option<ActionFn>,
    indent: TreeIndent,
    tree_column_key: Option<SharedString>,
    row_click_expansion: bool,
    tree_key: Rc<RefCell<Option<SharedString>>>,
    rows: RowLevels,
}

impl<T: 'static> TableTreeData<T> {
    /// Upstream `getRowMeta`, `onToggleItem` and `hasExpandableRows`
    /// (without expandable rows the plugin changes nothing).
    pub fn new(
        get_row_meta: impl Fn(&T) -> Option<TableTreeRowMeta> + 'static,
        on_toggle_item: impl Fn(&T, &mut Window, &mut App) + 'static,
        has_expandable_rows: bool,
    ) -> Self {
        Self {
            get_row_meta: Rc::new(get_row_meta),
            on_toggle_item: Rc::new(on_toggle_item),
            row_label: None,
            has_expandable_rows,
            expand_all_control: false,
            all_expanded: None,
            on_expand_all: None,
            on_collapse_all: None,
            indent: TreeIndent::Md,
            tree_column_key: None,
            row_click_expansion: false,
            tree_key: Rc::default(),
            rows: Rc::default(),
        }
    }

    /// Upstream `getRowLabel`: the expander is named "Expand" or "Collapse"
    /// and this (trimmed); blank keeps the generic name.
    pub fn row_label(mut self, f: impl Fn(&T) -> SharedString + 'static) -> Self {
        self.row_label = Some(Rc::new(f));
        self
    }

    /// Upstream `hasExpandAllControl`: a toggle in the tree column's header,
    /// with `all_expanded` and both handlers.
    pub fn expand_all_control(mut self, on: bool) -> Self {
        self.expand_all_control = on;
        self
    }

    /// Upstream `isAllExpanded`.
    pub fn all_expanded(mut self, state: AllExpanded) -> Self {
        self.all_expanded = Some(state);
        self
    }

    /// Upstream `onExpandAll`.
    pub fn on_expand_all(mut self, f: impl Fn(&mut Window, &mut App) + 'static) -> Self {
        self.on_expand_all = Some(Rc::new(f));
        self
    }

    /// Upstream `onCollapseAll`.
    pub fn on_collapse_all(mut self, f: impl Fn(&mut Window, &mut App) + 'static) -> Self {
        self.on_collapse_all = Some(Rc::new(f));
        self
    }

    /// Upstream `indent` (`Md` by default).
    pub fn indent(mut self, indent: TreeIndent) -> Self {
        self.indent = indent;
        self
    }

    /// Upstream `treeColumnKey`: the column with the indent and expander;
    /// the first column by default.
    pub fn tree_column_key(mut self, key: impl Into<SharedString>) -> Self {
        self.tree_column_key = Some(key.into());
        self
    }

    /// Upstream `hasRowClickExpansion`: a pointer click anywhere on an
    /// expandable row toggles it too.
    pub fn row_click_expansion(mut self, on: bool) -> Self {
        self.row_click_expansion = on;
        self
    }
}

impl<T> Clone for TableTreeData<T> {
    fn clone(&self) -> Self {
        Self {
            get_row_meta: self.get_row_meta.clone(),
            on_toggle_item: self.on_toggle_item.clone(),
            row_label: self.row_label.clone(),
            has_expandable_rows: self.has_expandable_rows,
            expand_all_control: self.expand_all_control,
            all_expanded: self.all_expanded,
            on_expand_all: self.on_expand_all.clone(),
            on_collapse_all: self.on_collapse_all.clone(),
            indent: self.indent,
            tree_column_key: self.tree_column_key.clone(),
            row_click_expansion: self.row_click_expansion,
            tree_key: self.tree_key.clone(),
            rows: self.rows.clone(),
        }
    }
}

/// Upstream `useTableTreeData`.
pub fn use_table_tree_data<T: 'static>(
    get_row_meta: impl Fn(&T) -> Option<TableTreeRowMeta> + 'static,
    on_toggle_item: impl Fn(&T, &mut Window, &mut App) + 'static,
    has_expandable_rows: bool,
) -> TableTreeData<T> {
    TableTreeData::new(get_row_meta, on_toggle_item, has_expandable_rows)
}

/// The chevron button of a row, or of the header's expand-all toggle.
#[derive(IntoElement)]
struct Expander {
    id: ElementId,
    label: SharedString,
    expanded: bool,
    /// The rows, its row's id and level: its child rows, while expanded.
    controls: Option<(RowLevels, SharedString, usize)>,
    on_toggle: ActionFn,
}

impl RenderOnce for Expander {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = current(window, cx);
        let t = theme.tokens();
        let rem = window.rem_size();
        let rtl = use_direction(cx).is_rtl();
        let compact = use_context::<CellDensity>(cx).is_some_and(|d| d.0 == TableDensity::Compact);
        let focus = control_focus(&self.id, window, cx);
        let radius = t.radius.inner.to_pixels(rem);
        let selector = format!("{}", self.id);
        let (hover_bg, hover_fg) = (t.color.overlay_hover, t.color.icon_primary);
        let toggle = self.on_toggle.clone();
        let mut el = div()
            .id(self.id)
            .debug_selector(move || selector)
            .track_focus(&focus)
            .relative()
            .flex()
            .flex_shrink_0()
            .items_center()
            .justify_center()
            .size(px(24.))
            .rounded(radius)
            .cursor_pointer()
            .text_color(t.color.icon_secondary)
            .hover(move |s| s.bg(hover_bg).text_color(hover_fg))
            .role(Role::Button)
            .aria_label(self.label)
            .aria_expanded(self.expanded)
            .on_click(move |_, window, cx| {
                cx.stop_propagation();
                toggle(window, cx)
            });
        // While expanded, the child rows it shows (rendered by now).
        if let Some((rows, id, level)) = self.controls
            && self.expanded
        {
            let rows = rows.borrow();
            let children: Vec<SharedString> = rows
                .iter()
                .skip_while(|(row, _, _)| *row != id)
                .skip(1)
                .take_while(|(_, l, _)| *l > level)
                .filter(|(_, l, _)| *l == level + 1)
                .map(|(_, _, name)| name.clone())
                .collect();
            if !children.is_empty() {
                el = el.aria_controls(children);
            }
        }
        if is_focus_visible(&focus, window) {
            // A compact row is shorter than the ring: it sits inside.
            el = el.child(if compact {
                crate::focus::focus_ring_inset(t, radius, rem)
            } else {
                crate::focus::focus_ring(t, radius, rem)
            });
        }
        let quarter = std::f32::consts::FRAC_PI_2;
        let turn = match (self.expanded, rtl) {
            (false, false) => 0.,
            (true, false) => quarter,
            // Mirrored: points left, down when open.
            (false, true) => std::f32::consts::PI,
            (true, true) => quarter,
        };
        el.child(Icon::new("chevronRight").size(IconSize::Xsm).rotation(turn))
    }
}

impl<T: 'static> TablePlugin<T> for TableTreeData<T> {
    fn row_model(&self) -> Option<RowModel> {
        Some(RowModel::Tree)
    }

    fn transforms_body(&self) -> bool {
        true
    }

    fn transform_columns(
        &self,
        columns: Vec<TableColumn<T>>,
        _: &mut Window,
        _: &mut App,
    ) -> Vec<TableColumn<T>> {
        self.rows.borrow_mut().clear();
        // The configured column when it is there (a column setting may hide
        // it), else the first that is not a plugin's own.
        let configured = self
            .tree_column_key
            .as_ref()
            .filter(|k| columns.iter().any(|c| &c.key == *k))
            .cloned();
        let key = configured.or_else(|| {
            columns
                .iter()
                .find(|c| !c.key.starts_with("__"))
                .or(columns.first())
                .map(|c| c.key.clone())
        });
        *self.tree_key.borrow_mut() = key.clone();
        // Flat data renders as without the plugin.
        if !self.has_expandable_rows {
            return columns;
        }
        let this = self.clone();
        columns
            .into_iter()
            .map(|mut col| {
                if Some(&col.key) == key.as_ref() {
                    let this = this.clone();
                    col.wrappers
                        .push(Rc::new(move |item: &TableItem<T>, content, window, cx| {
                            this.cell(item, content, window, cx)
                        }));
                }
                col
            })
            .collect()
    }

    fn transform_header_cell(
        &self,
        mut props: HeaderCellProps,
        column: &TableColumn<T>,
        _: &[TableColumn<T>],
        window: &mut Window,
        cx: &mut App,
    ) -> HeaderCellProps {
        let (Some(state), Some(expand), Some(collapse)) = (
            self.all_expanded,
            self.on_expand_all.clone(),
            self.on_collapse_all.clone(),
        ) else {
            return props;
        };
        if !self.expand_all_control
            || !self.has_expandable_rows
            || self.tree_key.borrow().as_ref() != Some(&column.key)
        {
            return props;
        }
        let all = state == AllExpanded::All;
        let t = i18n(cx);
        let toggle = Expander {
            id: ElementId::Name(format!("{}-tree-expand-all", column.key).into()),
            label: t.t(if all {
                "@astryx.tableTree.collapseAllRows"
            } else {
                "@astryx.tableTree.expandAllRows"
            }),
            expanded: all,
            controls: None,
            on_toggle: Rc::new(move |window, cx| {
                if all {
                    collapse(window, cx)
                } else {
                    expand(window, cx)
                }
            }),
        };
        let theme = current(window, cx);
        let gap = theme.tokens().spacing.s1.to_pixels(window.rem_size());
        // The toggle sits before the label on one line.
        props.content = Some(
            div()
                .flex()
                .items_center()
                .gap(gap)
                .min_w_0()
                .child(toggle)
                .children(props.content.take())
                .into_any_element(),
        );
        props
    }

    fn transform_body_row(
        &self,
        mut props: BodyRowProps,
        item: &TableItem<T>,
        context: &TableRowContext,
        _: &mut Window,
        _: &mut App,
    ) -> BodyRowProps {
        if !self.has_expandable_rows {
            return props;
        }
        let Some(meta) = (self.get_row_meta)(item) else {
            return props;
        };
        let name: SharedString = format!("{}-tree-row-{}", context.render.table, meta.id).into();
        self.rows
            .borrow_mut()
            .push((meta.id.clone(), meta.level, name.clone()));
        props.hooks.push(Box::new(move |row| row.aria_id(name)));
        // A pointer click on an expandable row toggles it too.
        if self.row_click_expansion && meta.has_children {
            let (toggle, row_item) = (self.on_toggle_item.clone(), item.clone());
            props.hooks.push(Box::new(move |row| {
                row.cursor_pointer().on_click(move |event, window, cx| {
                    if event.is_keyboard() || window.default_prevented() {
                        return;
                    }
                    toggle(&row_item, window, cx)
                })
            }));
        }
        props
    }
}

impl<T: 'static> TableTreeData<T> {
    /// Upstream `TreeCellContent`: the indent, the expander (or a spacer on
    /// a leaf) and the content.
    fn cell(
        &self,
        item: &TableItem<T>,
        content: AnyElement,
        window: &mut Window,
        cx: &mut App,
    ) -> AnyElement {
        let Some(meta) = (self.get_row_meta)(item) else {
            return content;
        };
        let theme = current(window, cx);
        let t = theme.tokens();
        let rem = window.rem_size();
        let dir = use_direction(cx);
        let step = match self.indent {
            TreeIndent::Sm => t.spacing.s3,
            TreeIndent::Md => t.spacing.s4,
            TreeIndent::Lg => t.spacing.s6,
        }
        .to_pixels(rem);
        let mut el = div().flex().items_center().gap(t.spacing.s1.to_pixels(rem));
        if meta.level > 0 {
            el = el.padding_start(dir, step * meta.level as f32);
        }
        let toggle_or_spacer = if meta.has_children {
            let tr = i18n(cx);
            let label = self
                .row_label
                .as_ref()
                .map(|f| f(item))
                .map(|l| SharedString::from(l.trim().to_string()))
                .filter(|l| !l.is_empty());
            let label = match (label, meta.is_expanded) {
                (Some(l), true) => tr.format(
                    "@astryx.tableTree.collapseRowNamed",
                    &[("label", l.as_ref().into())],
                ),
                (Some(l), false) => tr.format(
                    "@astryx.tableTree.expandRowNamed",
                    &[("label", l.as_ref().into())],
                ),
                (None, true) => tr.t("@astryx.tableTree.collapseRow"),
                (None, false) => tr.t("@astryx.tableTree.expandRow"),
            };
            let (toggle, row) = (self.on_toggle_item.clone(), item.clone());
            Expander {
                id: ElementId::Name(format!("tree-expander-{}", meta.id).into()),
                label,
                expanded: meta.is_expanded,
                controls: Some((self.rows.clone(), meta.id.clone(), meta.level)),
                on_toggle: Rc::new(move |window, cx| toggle(&row, window, cx)),
            }
            .into_any_element()
        } else {
            // Keeps a leaf's content in line with its expandable siblings.
            div().size(px(24.)).flex_shrink_0().into_any_element()
        };
        el.child(toggle_or_spacer).child(content).into_any_element()
    }
}

// ----------------------------------------------------------------------------
// useTableTreeState

/// What [`use_table_tree_state`] reads (upstream `UseTableTreeStateConfig`).
pub struct TreeStateConfig<T> {
    data: Rc<[T]>,
    id_key: IdKey<T>,
    children: ChildrenFn<T>,
    default_expanded: Vec<SharedString>,
    controlled: Option<Rc<HashSet<SharedString>>>,
    on_change: Option<IdsFn>,
    is_expandable: Option<Predicate<T>>,
    sort_siblings: Option<SortFn<T>>,
    indent: TreeIndent,
    tree_column_key: Option<SharedString>,
}

impl<T> TreeStateConfig<T> {
    /// Upstream `data` (nested), `idKey`, and `childrenKey` as the function
    /// that reads a row's children.
    pub fn new(
        data: impl Into<Rc<[T]>>,
        id_key: impl Into<IdKey<T>>,
        children: impl for<'a> Fn(&'a T) -> &'a [T] + 'static,
    ) -> Self {
        Self {
            data: data.into(),
            id_key: id_key.into(),
            children: Rc::new(children),
            default_expanded: Vec::new(),
            controlled: None,
            on_change: None,
            is_expandable: None,
            sort_siblings: None,
            indent: TreeIndent::Md,
            tree_column_key: None,
        }
    }

    /// Upstream `defaultExpandedIds`: the hook's own state to start.
    pub fn default_expanded_ids(
        mut self,
        ids: impl IntoIterator<Item = impl Into<SharedString>>,
    ) -> Self {
        self.default_expanded = ids.into_iter().map(Into::into).collect();
        self
    }

    /// Upstream `expandedIds`: a set the caller owns (with
    /// `on_expanded_ids_change`).
    pub fn expanded_ids(mut self, ids: impl IntoIterator<Item = SharedString>) -> Self {
        self.controlled = Some(Rc::new(ids.into_iter().collect()));
        self
    }

    /// Upstream `onExpandedIdsChange`: the next set, in both modes (fetch
    /// lazy children here).
    pub fn on_expanded_ids_change(
        mut self,
        f: impl Fn(HashSet<SharedString>, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_change = Some(Rc::new(f));
        self
    }

    /// Upstream `isItemExpandable`: instead of "has children" (lazy loading).
    pub fn is_item_expandable(mut self, f: impl Fn(&T) -> bool + 'static) -> Self {
        self.is_expandable = Some(Rc::new(f));
        self
    }

    /// Upstream `sortSiblings`: sorts each sibling group as it flattens
    /// (`SortableState::apply_sort`).
    pub fn sort_siblings(mut self, f: impl Fn(Vec<T>) -> Vec<T> + 'static) -> Self {
        self.sort_siblings = Some(Rc::new(f));
        self
    }

    /// Upstream `indent`.
    pub fn indent(mut self, indent: TreeIndent) -> Self {
        self.indent = indent;
        self
    }

    /// Upstream `treeColumnKey`.
    pub fn tree_column_key(mut self, key: impl Into<SharedString>) -> Self {
        self.tree_column_key = Some(key.into());
        self
    }
}

/// The hook's result (upstream `UseTableTreeStateResult`).
pub struct TreeState<T> {
    /// The rows to show: expanded subtrees flattened, collapsed ones left out.
    pub visible_data: Rc<[T]>,
    /// The plugin, configured (upstream `treeConfig`).
    pub plugin: TableTreeData<T>,
    pub expanded_ids: Rc<HashSet<SharedString>>,
    pub all_expanded: AllExpanded,
    expand_all: ActionFn,
    collapse_all: ActionFn,
}

impl<T> TreeState<T> {
    /// Upstream `expandAll`: every expandable row.
    pub fn expand_all(&self, window: &mut Window, cx: &mut App) {
        (self.expand_all)(window, cx)
    }

    /// Upstream `collapseAll`.
    pub fn collapse_all(&self, window: &mut Window, cx: &mut App) {
        (self.collapse_all)(window, cx)
    }
}

/// Upstream `useTableTreeState`: the expanded set (the hook's own under
/// `id`, or the caller's) and the visible rows.
pub fn use_table_tree_state<T: TableRecord + Clone>(
    id: impl Into<ElementId>,
    config: TreeStateConfig<T>,
    window: &mut Window,
    cx: &mut App,
) -> TreeState<T> {
    let id = id.into();
    let defaults = config.default_expanded.clone();
    let own = window.use_keyed_state(
        ElementId::Name(format!("{id:?}-tree-expanded").into()),
        cx,
        move |_, _: &mut Context<Rc<HashSet<SharedString>>>| {
            Rc::new(defaults.into_iter().collect())
        },
    );
    let controlled = config.controlled.is_some();
    let expanded: Rc<HashSet<SharedString>> = config
        .controlled
        .clone()
        .unwrap_or_else(|| own.read(cx).clone());
    let on_change = config.on_change.clone();
    let commit: IdsFn = Rc::new(move |next, window, cx| {
        if !controlled {
            own.update(cx, |ids, _| *ids = Rc::new(next.clone()));
        }
        if let Some(f) = &on_change {
            f(next, window, cx);
        }
        window.refresh();
    });
    let id_key = config.id_key.clone();
    let get_id = move |item: &T| id_key.string_key(item);
    let children = config.children.clone();
    let expandable = |item: &T| match &config.is_expandable {
        Some(f) => f(item),
        None => !children(item).is_empty(),
    };

    // Depth first, only the visible rows; an edge back to an ancestor is
    // skipped. Siblings are copied before sorting.
    let mut visible: Vec<T> = Vec::new();
    let mut metas: HashMap<SharedString, TableTreeRowMeta> = HashMap::new();
    let mut path: HashSet<SharedString> = HashSet::new();
    #[allow(clippy::too_many_arguments)]
    fn walk<T: Clone>(
        items: &[T],
        level: usize,
        get_id: &dyn Fn(&T) -> SharedString,
        expandable: &dyn Fn(&T) -> bool,
        children: &ChildrenFn<T>,
        sort: Option<&SortFn<T>>,
        expanded: &HashSet<SharedString>,
        path: &mut HashSet<SharedString>,
        visible: &mut Vec<T>,
        metas: &mut HashMap<SharedString, TableTreeRowMeta>,
    ) {
        let siblings: Vec<T> = match sort {
            Some(sort) => sort(items.to_vec()),
            None => items.to_vec(),
        };
        for item in siblings {
            let id = get_id(&item);
            if path.contains(&id) {
                continue;
            }
            let has_children = expandable(&item);
            let is_expanded = has_children && expanded.contains(&id);
            metas.insert(
                id.clone(),
                TableTreeRowMeta {
                    id: id.clone(),
                    level,
                    has_children,
                    is_expanded,
                },
            );
            if is_expanded {
                let kids = children(&item).to_vec();
                visible.push(item);
                path.insert(id.clone());
                walk(
                    &kids,
                    level + 1,
                    get_id,
                    expandable,
                    children,
                    sort,
                    expanded,
                    path,
                    visible,
                    metas,
                );
                path.remove(&id);
            } else {
                visible.push(item);
            }
        }
    }
    walk(
        &config.data,
        0,
        &get_id,
        &expandable,
        &children,
        config.sort_siblings.as_ref(),
        &expanded,
        &mut path,
        &mut visible,
        &mut metas,
    );
    // Every expandable id in the whole tree, collapsed subtrees too.
    let mut all_expandable: Vec<SharedString> = Vec::new();
    fn collect<T>(
        items: &[T],
        get_id: &dyn Fn(&T) -> SharedString,
        expandable: &dyn Fn(&T) -> bool,
        children: &ChildrenFn<T>,
        path: &mut HashSet<SharedString>,
        out: &mut Vec<SharedString>,
    ) {
        for item in items {
            let id = get_id(item);
            if path.contains(&id) {
                continue;
            }
            if expandable(item) {
                out.push(id.clone());
            }
            path.insert(id.clone());
            collect(children(item), get_id, expandable, children, path, out);
            path.remove(&id);
        }
    }
    collect(
        &config.data,
        &get_id,
        &expandable,
        &children,
        &mut HashSet::new(),
        &mut all_expandable,
    );
    let has_expandable_rows = !all_expandable.is_empty();
    let expanded_count = all_expandable
        .iter()
        .filter(|id| expanded.contains(*id))
        .count();
    let all_expanded = if expanded_count == 0 {
        AllExpanded::None
    } else if expanded_count == all_expandable.len() {
        AllExpanded::All
    } else {
        AllExpanded::Some
    };

    let metas = Rc::new(metas);
    let id_key = config.id_key.clone();
    let (toggle_set, toggle_commit) = (expanded.clone(), commit.clone());
    let toggle_key = config.id_key.clone();
    let (expand_commit, collapse_commit) = (commit.clone(), commit);
    let all_ids = all_expandable.clone();
    let expand_all: ActionFn =
        Rc::new(move |window, cx| expand_commit(all_ids.iter().cloned().collect(), window, cx));
    let collapse_all: ActionFn =
        Rc::new(move |window, cx| collapse_commit(HashSet::new(), window, cx));
    let (ea, ca) = (expand_all.clone(), collapse_all.clone());
    let mut plugin = TableTreeData::new(
        move |item: &T| metas.get(&id_key.string_key(item)).cloned(),
        move |item: &T, window, cx| {
            let id = toggle_key.string_key(item);
            let mut next = (*toggle_set).clone();
            if !next.remove(&id) {
                next.insert(id);
            }
            toggle_commit(next, window, cx)
        },
        has_expandable_rows,
    )
    .all_expanded(all_expanded)
    .on_expand_all(move |window, cx| ea(window, cx))
    .on_collapse_all(move |window, cx| ca(window, cx))
    .indent(config.indent);
    if let Some(key) = config.tree_column_key {
        plugin = plugin.tree_column_key(key);
    }
    TreeState {
        visible_data: visible.into(),
        plugin,
        expanded_ids: expanded,
        all_expanded,
        expand_all,
        collapse_all,
    }
}

const fn p(prop: &'static str, rust: RustApi) -> PropMapping {
    PropMapping { prop, rust }
}

pub const USE_TABLE_TREE_DATA_DOC: ComponentDoc = ComponentDoc {
    name: "useTableTreeData",
    rust_type: "astryx::components::table::TableTreeData",
    props: &[
        p("getRowMeta", RustApi::Argument("get_row_meta")),
        p("onToggleItem", RustApi::Argument("on_toggle_item")),
        p("getRowLabel", RustApi::Method("row_label")),
        p(
            "hasExpandableRows",
            RustApi::Argument("has_expandable_rows"),
        ),
        p("hasExpandAllControl", RustApi::Method("expand_all_control")),
        p("isAllExpanded", RustApi::Method("all_expanded")),
        p("onExpandAll", RustApi::Method("on_expand_all")),
        p("onCollapseAll", RustApi::Method("on_collapse_all")),
        p("indent", RustApi::Method("indent")),
        p("treeColumnKey", RustApi::Method("tree_column_key")),
        p(
            "hasRowClickExpansion",
            RustApi::Method("row_click_expansion"),
        ),
    ],
    example: include_str!("examples.rs"),
};

pub const USE_TABLE_TREE_STATE_DOC: ComponentDoc = ComponentDoc {
    name: "useTableTreeState",
    rust_type: "astryx::components::table::use_table_tree_state",
    props: &[
        p("data", RustApi::Argument("TreeStateConfig::new")),
        p("idKey", RustApi::Argument("TreeStateConfig::new")),
        p(
            "childrenKey",
            RustApi::Argument("TreeStateConfig::new (a function reading the children)"),
        ),
        p(
            "defaultExpandedIds",
            RustApi::Method("default_expanded_ids"),
        ),
        p("expandedIds", RustApi::Method("expanded_ids")),
        p(
            "onExpandedIdsChange",
            RustApi::Method("on_expanded_ids_change"),
        ),
        p("isItemExpandable", RustApi::Method("is_item_expandable")),
        p("sortSiblings", RustApi::Method("sort_siblings")),
        p("indent", RustApi::Method("indent")),
        p("treeColumnKey", RustApi::Method("tree_column_key")),
    ],
    example: include_str!("examples.rs"),
};
