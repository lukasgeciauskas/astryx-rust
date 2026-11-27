//! A table's right-click menus (upstream `tableContextMenu.tsx`): the
//! actions every plugin contributed to a header cell, or to a row's cells,
//! become one menu, grouped with a divider between groups in first-seen
//! order and ungrouped actions last. The actions are read when the menu
//! opens.

use std::rc::Rc;

use gpui::{App, Div, ElementId, SharedString, Stateful, Window};

use super::plugin::{TableContextAction, TableContextActions};
use crate::overlay::{MenuCheckboxItem, MenuEntry, MenuItem, MenuSize, PopoverPresentation};

/// The menu entries for `actions`.
pub(crate) fn entries(actions: Vec<TableContextAction>) -> Vec<MenuEntry> {
    let mut groups: Vec<(Option<SharedString>, Vec<TableContextAction>)> = Vec::new();
    let mut ungrouped = Vec::new();
    for action in actions {
        match action.group.clone() {
            None => ungrouped.push(action),
            Some(g) => match groups
                .iter_mut()
                .find(|(name, _)| name.as_ref() == Some(&g))
            {
                Some((_, list)) => list.push(action),
                None => groups.push((Some(g), vec![action])),
            },
        }
    }
    if !ungrouped.is_empty() {
        groups.push((None, ungrouped));
    }
    let mut out = Vec::new();
    for (i, (_, list)) in groups.into_iter().enumerate() {
        if i > 0 {
            out.push(MenuEntry::Divider);
        }
        for a in list {
            let select = a.on_select.clone();
            out.push(match a.checked {
                Some(checked) => {
                    let mut item = MenuCheckboxItem::new(a.id.clone(), a.label.clone(), checked)
                        .disabled(a.disabled)
                        .on_change(move |_, window, cx| select(window, cx));
                    if let Some(icon) = a.icon.clone() {
                        item = item.icon(icon);
                    }
                    MenuEntry::Checkbox(item)
                }
                None => {
                    let mut item = MenuItem::new(a.id.clone(), a.label.clone())
                        .disabled(a.disabled)
                        .destructive(a.destructive)
                        .on_click(move |window, cx| select(window, cx));
                    if let Some(icon) = a.icon.clone() {
                        item = item.icon(icon);
                    }
                    MenuEntry::Item(item)
                }
            });
        }
    }
    out
}

/// Gives `area` the menu of `actions`, when there are any (an empty list
/// leaves the platform's own behaviour alone).
pub(crate) fn with_actions(
    area: Stateful<Div>,
    id: ElementId,
    actions: Vec<TableContextActions>,
    label: Option<SharedString>,
    window: &mut Window,
    cx: &mut App,
) -> Stateful<Div> {
    let has_any = actions.iter().any(|a| match a {
        TableContextActions::List(l) => !l.is_empty(),
        TableContextActions::Lazy(_) => true,
    });
    if !has_any {
        return area;
    }
    let actions = Rc::new(actions);
    crate::overlay::with_context_menu(
        area,
        crate::overlay::ContextSpec {
            render: None,
            id,
            entries: Vec::new(),
            entries_fn: Some(Rc::new(move |_: &mut Window, cx: &mut App| {
                entries(actions.iter().flat_map(|a| a.resolve(cx)).collect())
            })),
            menu_width: None,
            size: MenuSize::Md,
            label,
            on_open_change: None,
            on_invoke: None,
            presentation: PopoverPresentation::Popover,
            disabled: false,
        },
        window,
        cx,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn action(id: &str, group: Option<&str>) -> TableContextAction {
        let a = TableContextAction::new(id.to_string(), id.to_string(), |_, _| {});
        match group {
            Some(g) => a.group(g.to_string()),
            None => a,
        }
    }

    #[test]
    fn groups_in_first_seen_order_with_ungrouped_last() {
        let e = entries(vec![
            action("plain", None),
            action("asc", Some("sort")),
            action("all", Some("selection")),
            action("desc", Some("sort")),
        ]);
        let shape: Vec<String> = e
            .iter()
            .map(|e| match e {
                MenuEntry::Item(i) => i.id.to_string(),
                MenuEntry::Divider => "|".into(),
                _ => "?".into(),
            })
            .collect();
        assert_eq!(shape, ["asc", "desc", "|", "all", "|", "plain"]);
    }
}
