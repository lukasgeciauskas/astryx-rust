//! A text field's context menu: what a browser shows on a right press in a
//! field, which GPUI has no counterpart for. Spelling suggestions for the
//! misspelled word under the pointer and "Add to dictionary", then Cut,
//! Copy, Paste and Select all, each enabled as the field allows.

use std::rc::Rc;

use gpui::{App, Div, ElementId, Entity, FocusHandle, IntoElement, Stateful, Window};
use rustryx_i18n::current as i18n;
use rustryx_primitives::editing::{FieldMenu, TextEditState};

use crate::content::Kbd;
use crate::overlay::{
    ContextSpec, MenuEntry, MenuItem, MenuSize, PopoverPresentation, with_context_menu,
};

/// How many suggestions the menu lists, as browsers do.
const SUGGESTIONS: usize = 5;

/// Gives a field's `control` its context menu, for the editor `state`.
pub(crate) fn with_field_menu(
    control: Stateful<Div>,
    id: &ElementId,
    state: &Entity<TextEditState>,
    focus: &FocusHandle,
    disabled: bool,
    window: &mut Window,
    cx: &mut App,
) -> Stateful<Div> {
    let found = window.use_keyed_state(
        ElementId::Name(format!("{id}-field-menu").into()),
        cx,
        |_, _| FieldMenu::default(),
    );
    // Invoked: the field takes focus (so focus comes back to it), then the
    // press places the caret or selects the misspelled word under it.
    let on_invoke = {
        let (state, found, focus) = (state.clone(), found.clone(), focus.clone());
        Rc::new(move |position, window: &mut Window, cx: &mut App| {
            window.focus(&focus, cx);
            let menu = state.update(cx, |s, cx| s.context_menu_at(position, window, cx));
            found.update(cx, |f, _| *f = menu);
        }) as crate::overlay::InvokeFn
    };
    let entries = {
        let (state, found) = (state.clone(), found.clone());
        Rc::new(move |_: &mut Window, cx: &mut App| field_entries(found.read(cx), &state, cx))
    };
    with_context_menu(
        control,
        ContextSpec {
            render: None,
            id: ElementId::Name(format!("{id}-menu").into()),
            entries: Vec::new(),
            entries_fn: Some(entries),
            menu_width: None,
            size: MenuSize::Md,
            label: None,
            on_open_change: None,
            on_invoke: Some(on_invoke),
            presentation: PopoverPresentation::Popover,
            disabled,
        },
        window,
        cx,
    )
}

fn field_entries(found: &FieldMenu, state: &Entity<TextEditState>, cx: &App) -> Vec<MenuEntry> {
    let t = i18n(cx);
    let mut entries = Vec::new();
    if let Some((range, word, suggestions)) = &found.misspelling {
        if suggestions.is_empty() {
            entries.push(
                MenuItem::new("no-suggestions", t.t("@astryx.textField.noSuggestions"))
                    .disabled(true)
                    .into(),
            );
        }
        for (i, suggestion) in suggestions.iter().take(SUGGESTIONS).enumerate() {
            let (state, range, with) = (state.clone(), range.clone(), suggestion.clone());
            entries.push(
                MenuItem::new(format!("suggestion-{i}"), suggestion.clone())
                    .on_click(move |window, cx| {
                        state.update(cx, |s, cx| {
                            s.replace_range(range.clone(), &with, window, cx)
                        })
                    })
                    .into(),
            );
        }
        let (state, word) = (state.clone(), word.clone());
        entries.push(
            MenuItem::new(
                "add-to-dictionary",
                t.t("@astryx.textField.addToDictionary"),
            )
            .on_click(move |_, cx| state.update(cx, |s, cx| s.add_to_dictionary(&word, cx)))
            .into(),
        );
        entries.push(MenuEntry::Divider);
    }
    let command = |id: &'static str, key: &str, shortcut: &'static str, enabled: bool| {
        let state = state.clone();
        MenuItem::new(id, t.t(key))
            .disabled(!enabled)
            .end_content(move |_, _| Kbd::new(shortcut).into_any_element())
            .on_click(move |window, cx| {
                state.update(cx, |s, cx| match id {
                    "cut" => s.copy(true, window, cx),
                    "copy" => s.copy(false, window, cx),
                    "paste" => s.paste(window, cx),
                    _ => s.select_all(window, cx),
                })
            })
    };
    entries.push(command("cut", "@astryx.textField.cut", "mod+x", found.can_cut).into());
    entries.push(command("copy", "@astryx.textField.copy", "mod+c", found.can_copy).into());
    entries.push(command("paste", "@astryx.textField.paste", "mod+v", found.can_paste).into());
    entries.push(MenuEntry::Divider);
    entries.push(
        command(
            "select-all",
            "@astryx.textField.selectAll",
            "mod+a",
            found.can_select_all,
        )
        .into(),
    );
    entries
}
