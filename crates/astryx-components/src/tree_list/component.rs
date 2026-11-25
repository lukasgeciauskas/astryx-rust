//! The TreeList component, upstream `TreeList.tsx`, `TreeListItem.tsx`
//! and `TreeListBranches.tsx`.

use std::collections::HashMap;
use std::rc::Rc;

use astryx_i18n::{current as i18n, use_direction};
use astryx_primitives::tree_focus::{TreeKey, TreeRow, tree_key, tree_typeahead};
use astryx_primitives::typeahead::is_shortcut_chord;
use astryx_primitives::{Typeahead, is_focus_visible, use_link_handler};
use astryx_theme::{ThemeStyled, current};
use gpui::{
    AnyElement, App, ClickEvent, ElementId, Entity, FocusHandle, IntoElement, KeyDownEvent,
    MouseButton, ParentElement, Pixels, RenderOnce, Role, SharedString, StyleRefinement, Styled,
    Window, div, prelude::*, px, relative,
};

use crate::focus::focus_ring;
use crate::icon::{Icon, IconColor, IconSize};
use crate::indicator::{IndicatorRequest, TREE_DISCLOSURE, theme_indicator};
use crate::motion::transition;

type SlotFn = Rc<dyn Fn(&mut Window, &mut App) -> AnyElement>;
type ClickFn = Rc<dyn Fn(&ClickEvent, &mut Window, &mut App)>;
type KeyFn = Rc<dyn Fn(&KeyDownEvent, &mut Window, &mut App)>;
type ToggleFn = Rc<dyn Fn(SharedString, &mut Window, &mut App)>;
type ActivateFn = Rc<dyn Fn(usize, bool, &ClickEvent, &mut Window, &mut App)>;

/// Upstream `density`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TreeListDensity {
    Compact,
    #[default]
    Balanced,
    Spacious,
}

impl TreeListDensity {
    pub fn as_str(self) -> &'static str {
        match self {
            TreeListDensity::Compact => "compact",
            TreeListDensity::Balanced => "balanced",
            TreeListDensity::Spacious => "spacious",
        }
    }
}

/// Upstream `variant`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TreeListVariant {
    /// Guide lines down each branch.
    #[default]
    LineGuides,
    NoGuides,
}

impl TreeListVariant {
    pub fn as_str(self) -> &'static str {
        match self {
            TreeListVariant::LineGuides => "lineGuides",
            TreeListVariant::NoGuides => "noGuides",
        }
    }
}

/// One node of the tree (upstream `TreeListItemData`).
#[derive(Clone)]
pub struct TreeListItemData {
    pub id: SharedString,
    /// The row's name and typeahead text, shown unless `label_content` is set.
    pub label: SharedString,
    /// Upstream's `label` node: rich content in the label's place.
    pub label_content: Option<SlotFn>,
    pub description: Option<SharedString>,
    pub start_content: Option<SlotFn>,
    pub end_content: Option<SlotFn>,
    pub children: Vec<TreeListItemData>,
    pub on_click: Option<ClickFn>,
    pub href: Option<SharedString>,
    pub target: Option<SharedString>,
    pub disabled: bool,
    pub selected: bool,
    /// Upstream `isExpanded`: open at first, until toggled.
    pub expanded: bool,
    /// Upstream `xstyle`, `className` and `style`: the row box (FR5, FR6).
    pub style: StyleRefinement,
}

impl TreeListItemData {
    pub fn new(id: impl Into<SharedString>, label: impl Into<SharedString>) -> Self {
        TreeListItemData {
            id: id.into(),
            label: label.into(),
            label_content: None,
            description: None,
            start_content: None,
            end_content: None,
            children: Vec::new(),
            on_click: None,
            href: None,
            target: None,
            disabled: false,
            selected: false,
            expanded: false,
            style: StyleRefinement::default(),
        }
    }

    /// Upstream `label` as a node: shown in the label's place; `label` stays
    /// the row's name.
    pub fn label_content(
        mut self,
        f: impl Fn(&mut Window, &mut App) -> AnyElement + 'static,
    ) -> Self {
        self.label_content = Some(Rc::new(f));
        self
    }

    /// Upstream `description`.
    pub fn description(mut self, description: impl Into<SharedString>) -> Self {
        self.description = Some(description.into());
        self
    }

    /// Upstream `startContent`.
    pub fn start_content(
        mut self,
        f: impl Fn(&mut Window, &mut App) -> AnyElement + 'static,
    ) -> Self {
        self.start_content = Some(Rc::new(f));
        self
    }

    /// Upstream `endContent`.
    pub fn end_content(
        mut self,
        f: impl Fn(&mut Window, &mut App) -> AnyElement + 'static,
    ) -> Self {
        self.end_content = Some(Rc::new(f));
        self
    }

    /// Upstream `children`.
    pub fn children(mut self, children: impl IntoIterator<Item = TreeListItemData>) -> Self {
        self.children = children.into_iter().collect();
        self
    }

    /// Upstream `onClick`.
    pub fn on_click(mut self, f: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static) -> Self {
        self.on_click = Some(Rc::new(f));
        self
    }

    /// Upstream `href`.
    pub fn href(mut self, href: impl Into<SharedString>) -> Self {
        self.href = Some(href.into());
        self
    }

    /// Upstream `target`.
    pub fn target(mut self, target: impl Into<SharedString>) -> Self {
        self.target = Some(target.into());
        self
    }

    /// Upstream `isDisabled`.
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    /// Upstream `isSelected`.
    pub fn selected(mut self, selected: bool) -> Self {
        self.selected = selected;
        self
    }

    /// Upstream `isExpanded`.
    pub fn expanded(mut self, expanded: bool) -> Self {
        self.expanded = expanded;
        self
    }

    /// Upstream `xstyle`: refines the row box.
    pub fn style(mut self, style: StyleRefinement) -> Self {
        self.style = style;
        self
    }

    fn has_children(&self) -> bool {
        !self.children.is_empty()
    }
}

struct TreeState {
    /// Rows the person expanded (true) or collapsed (false).
    overrides: HashMap<SharedString, bool>,
    /// The row with the tab stop, once focus moved.
    tab_stop: Option<SharedString>,
    typeahead: Typeahead,
    focus: HashMap<SharedString, FocusHandle>,
    /// The last press began on a control inside a row (FR7).
    press_in_control: bool,
}

/// One visible row.
struct Visible {
    item: TreeListItemData,
    level: usize,
    expanded: bool,
    is_last: bool,
    ancestors_last: Vec<bool>,
    pos: usize,
    set: usize,
}

/// Upstream `findInitialTabbableId`: the first selected enabled row (in
/// any branch), else the first enabled one.
fn initial_tab_stop(items: &[TreeListItemData]) -> Option<SharedString> {
    fn walk(items: &[TreeListItemData], first: &mut Option<SharedString>) -> Option<SharedString> {
        for item in items {
            if item.selected && !item.disabled {
                return Some(item.id.clone());
            }
            if first.is_none() && !item.disabled {
                *first = Some(item.id.clone());
            }
            if let Some(found) = walk(&item.children, first) {
                return Some(found);
            }
        }
        None
    }
    let mut first = None;
    walk(items, &mut first)
        .or(first)
        .or_else(|| items.first().map(|i| i.id.clone()))
}

/// A tree of nested items, upstream `TreeList`.
///
/// ```ignore
/// TreeList::new("files", [
///     TreeListItemData::new("src", "src").expanded(true).children([TreeListItemData::new("main", "main.rs")]),
/// ])
/// ```
#[derive(IntoElement)]
pub struct TreeList {
    id: ElementId,
    items: Vec<TreeListItemData>,
    density: TreeListDensity,
    variant: TreeListVariant,
    header: Option<AnyElement>,
    aria_label: Option<SharedString>,
    on_key_down: Option<KeyFn>,
    style: StyleRefinement,
}

impl TreeList {
    /// Upstream `items`.
    pub fn new(
        id: impl Into<ElementId>,
        items: impl IntoIterator<Item = TreeListItemData>,
    ) -> Self {
        TreeList {
            id: id.into(),
            items: items.into_iter().collect(),
            density: TreeListDensity::Balanced,
            variant: TreeListVariant::LineGuides,
            header: None,
            aria_label: None,
            on_key_down: None,
            style: StyleRefinement::default(),
        }
    }

    astryx_primitives::setters! {
        /// Upstream `density`.
        density: TreeListDensity,
        /// Upstream `variant`.
        variant: TreeListVariant,
    }

    /// Upstream `header`: above the tree.
    pub fn header(mut self, header: impl IntoElement) -> Self {
        self.header = Some(header.into_any_element());
        self
    }

    /// Upstream `aria-label`: the tree's name.
    pub fn aria_label(mut self, label: impl Into<SharedString>) -> Self {
        self.aria_label = Some(label.into());
        self
    }

    /// Upstream `onKeyDown`: before the tree's keys;
    /// `window.prevent_default()` keeps them from running.
    pub fn on_key_down(
        mut self,
        f: impl Fn(&KeyDownEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_key_down = Some(Rc::new(f));
        self
    }
}

impl Styled for TreeList {
    /// Upstream `xstyle`.
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl RenderOnce for TreeList {
    fn render(mut self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = current(window, cx);
        let t = theme.tokens();
        let rem = window.rem_size();
        let rtl = use_direction(cx).is_rtl();
        let state: Entity<TreeState> = window.use_keyed_state(
            ElementId::Name(format!("{}-tree", self.id).into()),
            cx,
            |_, _| TreeState {
                overrides: HashMap::new(),
                tab_stop: None,
                typeahead: tree_typeahead(),
                focus: HashMap::new(),
                press_in_control: false,
            },
        );
        // The visible rows, in order.
        let overrides = state.read(cx).overrides.clone();
        let is_expanded = |item: &TreeListItemData| {
            item.has_children() && overrides.get(&item.id).copied().unwrap_or(item.expanded)
        };
        let mut visible: Vec<Visible> = Vec::new();
        fn walk(
            items: &[TreeListItemData],
            level: usize,
            ancestors_last: &[bool],
            ancestors: &[SharedString],
            is_expanded: &dyn Fn(&TreeListItemData) -> bool,
            out: &mut Vec<Visible>,
        ) {
            for (index, item) in items.iter().enumerate() {
                let is_last = index + 1 == items.len();
                let expanded = is_expanded(item);
                out.push(Visible {
                    item: item.clone(),
                    level,
                    expanded,
                    is_last,
                    ancestors_last: ancestors_last.to_vec(),
                    pos: index + 1,
                    set: items.len(),
                });
                if expanded {
                    let mut last = ancestors_last.to_vec();
                    last.push(is_last);
                    let mut up = ancestors.to_vec();
                    up.push(item.id.clone());
                    walk(&item.children, level + 1, &last, &up, is_expanded, out);
                }
            }
        }
        walk(&self.items, 0, &[], &[], &is_expanded, &mut visible);
        let has_expandable = self.items.iter().any(TreeListItemData::has_children);

        // One focus handle per row; one tab stop.
        let handles: Vec<FocusHandle> = visible
            .iter()
            .map(|v| {
                let id = v.item.id.clone();
                state.update(cx, |s, cx| {
                    s.focus
                        .entry(id)
                        .or_insert_with(|| cx.focus_handle())
                        .clone()
                })
            })
            .collect();
        // AR2: focus in a group that collapsed goes to its nearest visible row.
        let lost = state
            .read(cx)
            .focus
            .iter()
            .find(|(id, h)| h.is_focused(window) && !visible.iter().any(|v| &v.item.id == *id))
            .map(|(id, _)| id.clone());
        if let Some(lost) = lost {
            let mut target = None;
            fn ancestors_of(
                items: &[TreeListItemData],
                id: &SharedString,
                path: &mut Vec<SharedString>,
            ) -> bool {
                for item in items {
                    if &item.id == id {
                        return true;
                    }
                    path.push(item.id.clone());
                    if ancestors_of(&item.children, id, path) {
                        return true;
                    }
                    path.pop();
                }
                false
            }
            let mut path = Vec::new();
            if ancestors_of(&self.items, &lost, &mut path) {
                target = path
                    .iter()
                    .rev()
                    .find_map(|a| visible.iter().position(|v| &v.item.id == a));
            }
            if let Some(at) = target {
                let (handle, id) = (handles[at].clone(), visible[at].item.id.clone());
                state.update(cx, |s, _| s.tab_stop = Some(id));
                window.defer(cx, move |window, cx| handle.focus(window, cx));
            }
        }
        let wanted = state
            .read(cx)
            .tab_stop
            .clone()
            .or_else(|| initial_tab_stop(&self.items));
        let tab_stop = wanted
            .and_then(|id| {
                visible
                    .iter()
                    .position(|v| v.item.id == id && !v.item.disabled)
            })
            .or_else(|| visible.iter().position(|v| !v.item.disabled));
        for (i, h) in handles.iter().enumerate() {
            let _ = h.clone().tab_stop(Some(i) == tab_stop);
        }

        // Toggling and activating.
        let toggle: ToggleFn = {
            let (state, defaults) = (
                state.clone(),
                visible
                    .iter()
                    .map(|v| (v.item.id.clone(), v.expanded))
                    .collect::<HashMap<_, _>>(),
            );
            Rc::new(
                move |id: SharedString, window: &mut Window, _cx: &mut App| {
                    let now = defaults.get(&id).copied().unwrap_or(false);
                    state.update(_cx, |s, _| {
                        s.overrides.insert(id, !now);
                    });
                    window.refresh();
                },
            )
        };
        let activate: ActivateFn = {
            let rows: Rc<Vec<(TreeListItemData, bool)>> = Rc::new(
                visible
                    .iter()
                    .map(|v| (v.item.clone(), v.item.has_children()))
                    .collect(),
            );
            let toggle = toggle.clone();
            Rc::new(
                move |index: usize,
                      modified: bool,
                      event: &ClickEvent,
                      window: &mut Window,
                      cx: &mut App| {
                    let (item, branch) = &rows[index];
                    if item.disabled {
                        return;
                    }
                    if let Some(f) = &item.on_click {
                        f(event, window, cx);
                    } else if *branch {
                        toggle(item.id.clone(), window, cx);
                    } else if let Some(href) = &item.href {
                        let target = if modified {
                            Some(SharedString::from("_blank"))
                        } else {
                            item.target.clone()
                        };
                        use_link_handler(cx).open(href, target, window, cx);
                    }
                },
            )
        };

        // The keys (AR1).
        let key_rows: Vec<TreeRow> = visible
            .iter()
            .map(|v| TreeRow {
                level: v.level + 1,
                expanded: v.item.has_children().then_some(v.expanded),
                disabled: v.item.disabled,
            })
            .collect();
        let labels: Vec<SharedString> = visible.iter().map(|v| v.item.label.clone()).collect();
        let on_key = {
            let (state, handles, toggle, activate, user) = (
                state.clone(),
                handles.clone(),
                toggle.clone(),
                activate.clone(),
                self.on_key_down.clone(),
            );
            let ids: Vec<SharedString> = visible.iter().map(|v| v.item.id.clone()).collect();
            let disabled: Vec<bool> = visible.iter().map(|v| v.item.disabled).collect();
            move |event: &KeyDownEvent, window: &mut Window, cx: &mut App| {
                if let Some(f) = &user {
                    f(event, window, cx);
                    if window.default_prevented() {
                        return;
                    }
                }
                let current = handles.iter().position(|h| h.is_focused(window));
                let focus_row = |at: usize, window: &mut Window, cx: &mut App| {
                    state.update(cx, |s, _| s.tab_stop = Some(ids[at].clone()));
                    handles[at].focus(window, cx);
                    window.refresh();
                };
                let key = event.keystroke.key.as_str();
                // Typeahead over the labels, from the row after this one.
                let typed = key.chars().count() == 1
                    && !is_shortcut_chord(&event.keystroke)
                    && key != " "
                    && !matches!(
                        key,
                        "up" | "down" | "left" | "right" | "home" | "end" | "enter" | "space"
                    );
                if typed {
                    let found = state.update(cx, |s, _| {
                        s.typeahead.key(
                            &event.keystroke,
                            labels.len(),
                            |i| labels.get(i).map(|l| l.as_ref()),
                            current,
                            |i| disabled[i],
                        )
                    });
                    if let Some(found) = found {
                        if let Some(at) = found {
                            focus_row(at, window, cx);
                        }
                        cx.stop_propagation();
                    }
                    return;
                }
                match tree_key(&key_rows, current, key, rtl) {
                    TreeKey::Ignored => return,
                    TreeKey::Handled => {}
                    TreeKey::Focus(at) => focus_row(at, window, cx),
                    TreeKey::Toggle(at) => toggle(ids[at].clone(), window, cx),
                    TreeKey::Activate(at) => {
                        let event = ClickEvent::default();
                        activate(at, false, &event, window, cx);
                    }
                }
                cx.stop_propagation();
            }
        };

        // The rows.
        let indent = t.spacing.s4.to_pixels(rem);
        let row_gap = t.spacing.s0_5.to_pixels(rem);
        let chevron_box = t.spacing.s4.to_pixels(rem);
        let (pad_y, desc_size, desc_leading) = match self.density {
            TreeListDensity::Compact => (
                t.spacing.s1,
                t.text.supporting.size,
                t.text.supporting.leading,
            ),
            TreeListDensity::Balanced => (
                t.spacing.s2,
                t.text.supporting.size,
                t.text.supporting.leading,
            ),
            TreeListDensity::Spacious => (
                t.spacing.s3,
                t.text.supporting.size,
                t.text.supporting.leading,
            ),
        };
        let radius = t.radius.element.to_pixels(rem);
        let disclosure = theme_indicator(TREE_DISCLOSURE, cx);
        let mut rows: Vec<AnyElement> = Vec::new();
        for (index, v) in visible.iter().enumerate() {
            let item = &v.item;
            let handle = handles[index].clone();
            let branch = item.has_children();
            let interactive = item.on_click.is_some() || item.href.is_some() || branch;
            let keyboard_focus = is_focus_visible(&handle, window);
            let selector = format!("{}-row-{}", self.id, item.id);
            // Guides (FR1): one line per open ancestor column, and the row's own.
            let guide_x = |level: usize| {
                t.spacing.s2.to_pixels(rem) + chevron_box / 2. + indent * level as f32
            };
            let mut guides: Vec<AnyElement> = Vec::new();
            if self.variant == TreeListVariant::LineGuides {
                let line = |x: Pixels, last: bool| {
                    let mut g = div()
                        .absolute()
                        .top_0()
                        .w(px(1.))
                        .rounded(px(1.))
                        .bg(t.color.border_emphasized);
                    g = if last {
                        g.h(relative(1.)).mb(row_gap / 2.)
                    } else {
                        g.h(relative(1.)).mb(px(-1.))
                    };
                    g = if rtl { g.right(x) } else { g.left(x) };
                    theme.refine(g.style(), "tree-list-guide", &[], None);
                    g.into_any_element()
                };
                for level in 0..v.ancestors_last.len() {
                    let continues = level + 1 < v.level
                        && !v.ancestors_last.get(level + 1).copied().unwrap_or(true);
                    if continues {
                        guides.push(line(guide_x(level), false));
                    }
                }
                if v.level > 0 {
                    guides.push(line(guide_x(v.level - 1), v.is_last));
                }
            }
            // The chevron (FR10: or the theme's disclosure mark).
            let chevron = branch.then(|| {
                let state_name = if v.expanded { "expanded" } else { "collapsed" };
                let mark: AnyElement = match &disclosure {
                    Some(draw) => draw(
                        IndicatorRequest {
                            state: state_name,
                            size: chevron_box,
                        },
                        window,
                        cx,
                    ),
                    None => {
                        let turn = transition(
                            format!("{}-chevron-{}", self.id, item.id),
                            v.expanded,
                            t.duration.fast,
                            t.ease.standard,
                            window,
                            cx,
                        );
                        let base = if rtl { std::f32::consts::PI } else { 0. };
                        let quarter = std::f32::consts::FRAC_PI_2 * if rtl { -1. } else { 1. };
                        Icon::new("chevronRight")
                            .size(IconSize::Sm)
                            .color(IconColor::Inherit)
                            .rotation(base + quarter * turn)
                            .into_any_element()
                    }
                };
                let (toggle, id, disabled) = (toggle.clone(), item.id.clone(), item.disabled);
                let chevron_selector = format!("{}-chevron-{}", self.id, item.id);
                let mut c = div()
                    .id(ElementId::Name(
                        format!("{}-toggle-{}", self.id, item.id).into(),
                    ))
                    .debug_selector(move || chevron_selector)
                    .flex()
                    .flex_none()
                    .items_center()
                    .justify_center()
                    .size(chevron_box)
                    .rounded(t.radius.inner)
                    .text_color(t.color.icon_secondary)
                    .role(Role::Button)
                    .aria_label(i18n(cx).t("@astryx.treeList.toggleChildren"))
                    .aria_expanded(v.expanded)
                    .aria_disabled(disabled)
                    .child(mark);
                if !disabled {
                    c = c.cursor_pointer().on_click(move |_, window, cx| {
                        cx.stop_propagation();
                        toggle(id.clone(), window, cx);
                    });
                }
                theme.refine(
                    c.style(),
                    "tree-list-chevron",
                    &[("state", state_name)],
                    None,
                );
                c
            });
            let reserves = !branch && has_expandable;
            let pad_start = indent * v.level as f32
                + if reserves {
                    chevron_box + t.spacing.s2.to_pixels(rem)
                } else {
                    px(0.)
                };
            let mut label =
                div()
                    .text_color(t.color.text_primary)
                    .child(match &item.label_content {
                        Some(f) => f(window, cx),
                        None => item.label.clone().into_any_element(),
                    });
            theme.refine(
                label.style(),
                "tree-list-item-label",
                &if item.selected {
                    vec![("selected", "selected")]
                } else {
                    vec![]
                },
                None,
            );
            let text = div()
                .flex()
                .flex_col()
                .flex_1()
                .min_w_0()
                .child(label)
                .children(item.description.clone().map(|d| {
                    div()
                        .text_color(t.color.text_secondary)
                        .text_size(desc_size)
                        .line_height(relative(desc_leading))
                        .child(d)
                }));
            let inner = div()
                .flex()
                .flex_1()
                .min_w_0()
                .items_center()
                .gap(t.spacing.s2)
                .pl(pad_start)
                .children(chevron)
                .children(
                    item.start_content
                        .as_ref()
                        .map(|f| div().flex().flex_none().items_center().child(f(window, cx))),
                )
                .child(text)
                .children(item.end_content.as_ref().map(|f| {
                    div()
                        .flex()
                        .flex_none()
                        .items_center()
                        .ml_auto()
                        .child(f(window, cx))
                }));
            let mut props: Vec<(&'static str, &'static str)> =
                vec![("density", self.density.as_str())];
            if item.selected {
                props.push(("selected", "selected"));
            }
            if item.disabled {
                props.push(("disabled", "disabled"));
            }
            let mut content = div()
                .id(ElementId::Name(
                    format!("{}-content-{}", self.id, item.id).into(),
                ))
                .relative()
                .flex()
                .items_center()
                .gap(t.spacing.s2)
                .px(t.spacing.s2)
                .py(pad_y)
                .rounded(radius)
                .overflow_hidden()
                .text_size(t.text.body.size)
                .line_height(relative(t.text.body.leading))
                .child(inner);
            if item.selected {
                content = content.bg(t.color.accent_muted);
            }
            if item.disabled {
                content = content.opacity(0.5).cursor_default();
            } else if interactive {
                let hover = t.color.overlay_hover;
                let pressed = t.color.overlay_pressed;
                let base = if item.selected {
                    t.color.accent_muted
                } else {
                    gpui::transparent_black()
                };
                content = content
                    .cursor_pointer()
                    .hover(move |s| s.bg(astryx_theme::color::composite(base, hover)))
                    .active(move |s| s.bg(astryx_theme::color::composite(base, pressed)));
                // A press that a control inside the row took (it focused,
                // preventing the default) is the control's, not the row's.
                {
                    let state = state.clone();
                    content = content.on_mouse_down(MouseButton::Left, move |_, window, cx| {
                        let taken = window.default_prevented();
                        state.update(cx, |s, _| s.press_in_control = taken);
                    });
                }
                let (act, pressed) = (activate.clone(), state.clone());
                content = content.on_click(move |event, window, cx| {
                    if !event.is_keyboard() && pressed.read(cx).press_in_control {
                        return;
                    }
                    let m = event.modifiers();
                    act(index, m.platform || m.control || m.shift, event, window, cx)
                });
                if item.href.is_some() && item.on_click.is_none() && !branch {
                    let act = activate.clone();
                    content = content.on_mouse_up(MouseButton::Middle, move |_, window, cx| {
                        act(index, true, &ClickEvent::default(), window, cx)
                    });
                }
            }
            if keyboard_focus {
                content = content.child(focus_ring(t, radius, rem));
            }
            theme.refine(content.style(), "tree-list-item", &props, None);
            content.style().refine(&item.style);
            let row = div()
                .id(ElementId::Name(
                    format!("{}-item-{}", self.id, item.id).into(),
                ))
                .debug_selector(move || selector)
                .relative()
                .w_full()
                .py(row_gap / 2.)
                .track_focus(&handle)
                .role(Role::TreeItem)
                .aria_label(item.label.clone())
                .aria_level(v.level + 1)
                .aria_position_in_set(v.pos)
                .aria_size_of_set(v.set)
                .aria_disabled(item.disabled)
                .when_some(item.description.clone(), |el, d| el.aria_description(d))
                .when(branch, |el| el.aria_expanded(v.expanded))
                .when(item.selected, |el| el.aria_selected(true))
                .on_mouse_down(MouseButton::Left, {
                    let (state, id, disabled) = (state.clone(), item.id.clone(), item.disabled);
                    move |_, _, cx| {
                        if !disabled {
                            state.update(cx, |s, _| s.tab_stop = Some(id.clone()));
                        }
                    }
                })
                .children(guides)
                .child(content);
            rows.push(row.into_any_element());
        }

        let mut root = div()
            .id(self.id.clone())
            .relative()
            .flex()
            .flex_col()
            .font_stack(&t.font.family.body);
        theme.refine(
            root.style(),
            "tree-list",
            &[
                ("density", self.density.as_str()),
                ("variant", self.variant.as_str()),
            ],
            None,
        );
        root.style().refine(&self.style);
        let header = self.header.take().map(|h| div().mb(t.spacing.s2).child(h));
        let tree_selector = format!("{}-tree", self.id);
        let mut tree = div()
            .id(ElementId::Name(format!("{}-tree", self.id).into()))
            .debug_selector(move || tree_selector)
            .flex()
            .flex_col()
            .role(Role::Tree)
            .on_key_down(on_key)
            .children(rows);
        if let Some(label) = self.aria_label.clone() {
            tree = tree.aria_label(label);
        }
        root.children(header).child(tree)
    }
}
