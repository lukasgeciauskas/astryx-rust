//! The search row of a selector's panel, upstream `PanelSearchInput`:
//! a magnifier, a borderless field and, while a query shows, a clear
//! button. Selector and MultiSelector share it.

use std::rc::Rc;

use astryx_i18n::current as i18n;
use astryx_primitives::editing::{EditorStyle, FieldSettings, TextEditState, text_editor};
use astryx_primitives::{
    ElementSize, FocusNext, FocusPrevious, is_focus_visible, prefers_reduced_motion,
};
use astryx_theme::{ThemeStyled, current};
use gpui::{
    AnyElement, App, Div, ElementId, Entity, Focusable, KeyDownEvent, ParentElement, Pixels, Role,
    SharedString, Stateful, Styled, Window, div, point, prelude::*, px, relative,
};

use crate::button::{ButtonVariant, IconButton};
use crate::icon::{Icon, IconColor, IconSize};

pub(crate) type QueryFn = Rc<dyn Fn(String, &mut Window, &mut App)>;
pub(crate) type SearchKeyFn = Rc<dyn Fn(&KeyDownEvent, &mut Window, &mut App) -> bool>;
pub(crate) type CloseFn = Rc<dyn Fn(&mut Window, &mut App)>;

/// What one panel's search row needs.
pub(crate) struct PanelSearch {
    /// The component's id: the row is `{id}-search-row`, the clear button
    /// `{id}-search-clear`.
    pub id: ElementId,
    pub editor: Entity<TextEditState>,
    pub has_query: bool,
    /// The field's name (upstream `searchOptions`).
    pub label: SharedString,
    pub placeholder: SharedString,
    pub required: bool,
    pub pad_x: Pixels,
    /// The theme target (`selector-search`, `multi-selector-search`).
    pub target: &'static str,
    /// Each edit of the query, and the clear button's empty one.
    pub on_query: QueryFn,
    /// Every key in the field: true when handled.
    pub on_key: SearchKeyFn,
    /// Tab out of the panel.
    pub on_close: CloseFn,
    /// Moves focus into the field once drawn.
    pub focus_probe: AnyElement,
}

/// The row, before the panel places it.
pub(crate) fn panel_search(p: PanelSearch, window: &mut Window, cx: &mut App) -> Stateful<Div> {
    let theme = current(window, cx);
    let t = theme.tokens();
    let t18 = i18n(cx);
    let editor = p.editor.clone();
    let search_focus = editor.read(cx).focus_handle(cx);
    {
        let (on_query, on_key) = (p.on_query.clone(), p.on_key.clone());
        editor.update(cx, |e, _| {
            e.set_settings(FieldSettings {
                spell_check: false,
                ..FieldSettings::default()
            });
            e.on_change = Some(Rc::new(
                move |text: String, window: &mut Window, cx: &mut App| on_query(text, window, cx),
            ));
            e.on_key_down = Some(Rc::new(
                move |event: &KeyDownEvent, window: &mut Window, cx: &mut App| {
                    if on_key(event, window, cx) {
                        cx.stop_propagation();
                    }
                },
            ));
        });
    }
    let style = EditorStyle {
        text_color: t.color.text_primary,
        placeholder: Some(p.placeholder.clone()),
        placeholder_color: t.color.text_secondary,
        selection_color: crate::input::selection_color(t),
        caret_color: t.color.text_primary,
        multiline: false,
        min_lines: 1,
        max_lines: Some(1),
        height: None,
        reduced_motion: prefers_reduced_motion(cx),
        spelling_color: t.color.error,
    };
    let field = text_editor("query", &editor, style, cx)
        .flex_1()
        .min_w_0()
        .font_stack(&t.font.family.body)
        .text_size(t.text.label.size)
        .line_height(relative(t.text.label.leading))
        .role(Role::ComboBox)
        .aria_label(p.label.clone())
        .aria_expanded(true)
        .aria_required(p.required);
    let clear = p.has_query.then(|| {
        let (on_query, editor, focus) = (p.on_query.clone(), editor.clone(), search_focus.clone());
        let selector = format!("{}-search-clear", p.id);
        div().debug_selector(move || selector).child(
            IconButton::new(
                ElementId::Name(format!("{}-search-clear", p.id).into()),
                t18.format(
                    "@astryx.textInput.clearLabel",
                    &[("label", p.label.as_ref().into())],
                ),
                Icon::new("close")
                    .size(IconSize::Sm)
                    .color(IconColor::Secondary),
            )
            .variant(ButtonVariant::Ghost)
            .size(ElementSize::Sm)
            .h(px(20.))
            .on_click(move |_, window, cx| {
                cx.stop_propagation();
                editor.update(cx, |e, cx| e.set_text("", cx));
                on_query(String::new(), window, cx);
                window.focus(&focus, cx);
            }),
        )
    });
    let mut input_box = div()
        .relative()
        .flex()
        .items_center()
        .gap(t.spacing.s2)
        .w_full()
        .py(t.spacing.s1_5)
        .px(t.spacing.s2)
        .rounded(t.radius.element)
        .child(
            Icon::new("search")
                .size(IconSize::Sm)
                .color(IconColor::Secondary),
        )
        .child(field)
        .children(clear);
    // The keyboard's focus ring, inset (the field sits inside the panel).
    if is_focus_visible(&search_focus, window) {
        input_box = input_box.shadow(vec![gpui::BoxShadow {
            color: t.color.accent,
            offset: point(px(0.), px(0.)),
            blur_radius: px(0.),
            spread_radius: px(2.),
            inset: true,
        }]);
    }
    let mut row = div()
        .id(ElementId::Name(format!("{}-search-row", p.id).into()))
        .flex_none()
        .py(t.spacing.s1)
        .px(p.pad_x)
        .child(input_box)
        .child(p.focus_probe);
    theme.refine(row.style(), p.target, &[], None);
    // Tab: to the clear button while a query shows it, else out of the
    // closing list (from the trigger, as upstream's focus return does).
    {
        let (close, focus, has_query) = (p.on_close.clone(), search_focus.clone(), p.has_query);
        row = row.on_action(move |_: &FocusNext, window, cx| {
            if !(focus.is_focused(window) && has_query) {
                close(window, cx);
            }
            cx.propagate();
        });
    }
    {
        let (close, focus) = (p.on_close.clone(), search_focus.clone());
        row = row.on_action(move |_: &FocusPrevious, window, cx| {
            if focus.is_focused(window) {
                close(window, cx);
            }
            cx.propagate();
        });
    }
    row
}
