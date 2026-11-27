//! `ChatEmojiPicker`: a popover emoji grid for chat reactions that wraps its trigger
//! (upstream lab `ChatEmojiPicker`): a shortname filter over a fixed 8-column grid. The
//! arrow keys move focus through the grid (left and right by one, up and down by a row,
//! with EmojiPicker's grid keys, upstream `useGridFocus`); picking an emoji calls
//! `on_select` and closes the popover, which returns focus to the trigger. Ships with a
//! small default set; `emojis` overrides it.

use std::collections::HashMap;
use std::rc::Rc;

use astryx_components::input::TextInput;
use astryx_components::overlay::Popover;
use astryx_primitives::{ElementSize, LayerAlign, Placement, is_focus_visible};
use astryx_theme::{ThemeStyled, current};
use gpui::prelude::*;
use gpui::{
    AnyElement, App, ElementId, FocusHandle, IntoElement, KeyDownEvent, Role, SharedString,
    StyleRefinement, Window, div, px,
};

use crate::emoji_picker::grid::{GridKey, GridMove, grid_move};

/// One emoji the picker offers (upstream `ChatEmojiOption`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChatEmojiOption {
    /// The emoji character, e.g. "🎉".
    pub emoji: SharedString,
    /// The shortname used for filtering and accessible labels, e.g. "tada".
    pub name: SharedString,
}

impl ChatEmojiOption {
    /// An emoji and its shortname.
    pub fn new(emoji: impl Into<SharedString>, name: impl Into<SharedString>) -> Self {
        ChatEmojiOption {
            emoji: emoji.into(),
            name: name.into(),
        }
    }
}

/// The small default set (two grid rows) covering the common reaction vocabulary
/// (upstream `DEFAULT_CHAT_EMOJIS`).
pub fn default_chat_emojis() -> Vec<ChatEmojiOption> {
    [
        ("👍", "thumbs up"),
        ("❤️", "heart"),
        ("😂", "joy"),
        ("🎉", "tada"),
        ("😮", "wow"),
        ("😢", "cry"),
        ("🔥", "fire"),
        ("👀", "eyes"),
        ("✅", "check"),
        ("🙏", "pray"),
        ("💯", "hundred"),
        ("🚀", "rocket"),
        ("😍", "heart eyes"),
        ("🤔", "thinking"),
        ("👋", "wave"),
        ("⭐", "star"),
    ]
    .into_iter()
    .map(|(e, n)| ChatEmojiOption::new(e, n))
    .collect()
}

/// The grid's columns (upstream `GRID_COLUMNS`).
pub const GRID_COLUMNS: usize = 8;

/// The options a filter shows (upstream's `visible`): all of them for an empty query, else
/// those whose shortname holds the trimmed, lower-cased query, or whose emoji is it.
pub fn visible_chat_emojis<'a>(
    emojis: &'a [ChatEmojiOption],
    query: &str,
) -> Vec<&'a ChatEmojiOption> {
    let trimmed = query.trim().to_lowercase();
    if trimmed.is_empty() {
        return emojis.iter().collect();
    }
    emojis
        .iter()
        .filter(|o| o.name.to_lowercase().contains(&trimmed) || o.emoji.as_ref() == trimmed)
        .collect()
}

/// The message an empty filter result shows (upstream's, untranslated): "No emoji match
/// “query”."
pub fn no_chat_emoji_match(query: &str) -> SharedString {
    format!("No emoji match \u{201c}{}\u{201d}.", query.trim()).into()
}

type SelectFn = Rc<dyn Fn(SharedString, &mut Window, &mut App)>;

/// The reaction emoji popover (upstream lab `ChatEmojiPicker`).
///
/// ```ignore
/// ChatEmojiPicker::new("react").on_select(|emoji, _, _| { /* add it */ }).child(trigger)
/// ```
#[derive(IntoElement)]
pub struct ChatEmojiPicker {
    id: ElementId,
    on_select: Option<SelectFn>,
    emojis: Option<Vec<ChatEmojiOption>>,
    label: SharedString,
    search_label: SharedString,
    trigger: Option<AnyElement>,
    style: StyleRefinement,
}

impl ChatEmojiPicker {
    /// A picker over [`default_chat_emojis`].
    pub fn new(id: impl Into<ElementId>) -> Self {
        ChatEmojiPicker {
            id: id.into(),
            on_select: None,
            emojis: None,
            label: "Pick an emoji".into(),
            search_label: "Search emoji".into(),
            trigger: None,
            style: StyleRefinement::default(),
        }
    }

    /// Upstream `onSelect`: the picked emoji character; the popover closes itself after.
    pub fn on_select(mut self, f: impl Fn(SharedString, &mut Window, &mut App) + 'static) -> Self {
        self.on_select = Some(Rc::new(f));
        self
    }

    /// Upstream `emojis`: the options in the grid, 8 a row.
    pub fn emojis(mut self, emojis: impl IntoIterator<Item = ChatEmojiOption>) -> Self {
        self.emojis = Some(emojis.into_iter().collect());
        self
    }

    /// Upstream `label`: the popover dialog's accessible name ("Pick an emoji").
    pub fn label(mut self, label: impl Into<SharedString>) -> Self {
        self.label = label.into();
        self
    }

    /// Upstream `searchLabel`: the filter's placeholder and hidden label ("Search emoji").
    pub fn search_label(mut self, label: impl Into<SharedString>) -> Self {
        self.search_label = label.into();
        self
    }

    /// Upstream `children`: the trigger, a Button (or anything built on it), which the
    /// popover wires up.
    pub fn child(mut self, trigger: impl IntoElement) -> Self {
        self.trigger = Some(trigger.into_any_element());
        self
    }
}

impl Styled for ChatEmojiPicker {
    /// Upstream `xstyle`: the popover's panel.
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

/// What a picker keeps: whether it is open, the filter and each emoji's focus.
#[derive(Default)]
struct PickerState {
    open: bool,
    query: SharedString,
    focus: HashMap<SharedString, FocusHandle>,
}

/// What a picker drew, for tests.
#[cfg(test)]
#[derive(Clone, Debug, Default)]
pub(crate) struct PickerDrawn {
    pub open: bool,
    pub cells: Vec<(SharedString, SharedString)>,
    pub empty: Option<SharedString>,
    pub handles: Vec<FocusHandle>,
    pub filter_id: Option<ElementId>,
}

#[cfg(test)]
thread_local! {
    pub(crate) static PICKERS: std::cell::RefCell<HashMap<String, PickerDrawn>> = Default::default();
}

impl RenderOnce for ChatEmojiPicker {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let state = window.use_keyed_state(
            ElementId::Name(format!("{}-chat-emoji", self.id).into()),
            cx,
            |_, _| PickerState::default(),
        );
        let emojis = self.emojis.clone().unwrap_or_else(default_chat_emojis);
        let open = state.read(cx).open;
        #[cfg(test)]
        PICKERS.with(|p| {
            p.borrow_mut().insert(
                self.id.to_string(),
                PickerDrawn {
                    open,
                    ..Default::default()
                },
            )
        });
        // Closing clears the filter.
        let set_open = {
            let state = state.clone();
            Rc::new(move |open: bool, cx: &mut App| {
                state.update(cx, |s, cx| {
                    s.open = open;
                    if !open {
                        s.query = SharedString::default();
                    }
                    cx.notify();
                })
            })
        };
        let content = {
            let (state, set_open) = (state.clone(), set_open.clone());
            let (id, search_label, on_select, style) = (
                self.id.clone(),
                self.search_label.clone(),
                self.on_select.clone(),
                self.style.clone(),
            );
            move |window: &mut Window, cx: &mut App| -> AnyElement {
                let theme = current(window, cx);
                let t = theme.tokens().clone();
                let rem = window.rem_size();
                let query = state.read(cx).query.clone();
                let visible: Vec<ChatEmojiOption> = visible_chat_emojis(&emojis, &query)
                    .into_iter()
                    .cloned()
                    .collect();
                let handles: Vec<FocusHandle> = state.update(cx, |s, cx| {
                    visible
                        .iter()
                        .map(|o| {
                            s.focus
                                .entry(o.emoji.clone())
                                .or_insert_with(|| cx.focus_handle())
                                .clone()
                        })
                        .collect()
                });
                let change = state.clone();
                let filter_id = ElementId::Name(format!("{id}-filter").into());
                let input = TextInput::new(filter_id.clone(), search_label.clone(), query.clone())
                    .label_hidden(true)
                    .size(ElementSize::Sm)
                    .placeholder(search_label.clone())
                    .clear(true)
                    .on_change(move |value, _, cx| {
                        change.update(cx, |s, cx| {
                            s.query = value;
                            cx.notify();
                        })
                    });
                #[cfg(test)]
                PICKERS.with(|p| {
                    if let Some(d) = p.borrow_mut().get_mut(&id.to_string()) {
                        d.cells = visible
                            .iter()
                            .map(|o| (o.emoji.clone(), format!("React with {}", o.name).into()))
                            .collect();
                        d.empty = visible.is_empty().then(|| no_chat_emoji_match(&query));
                        d.handles = handles.clone();
                        d.filter_id = Some(filter_id.clone());
                    }
                });
                let body: AnyElement = if visible.is_empty() {
                    div()
                        .py(t.spacing.s2)
                        .px(t.spacing.s1)
                        .font_stack(&t.font.family.body)
                        .text_size(t.text.supporting.size)
                        .text_color(t.color.text_secondary)
                        .child(no_chat_emoji_match(&query))
                        .into_any_element()
                } else {
                    let rtl = astryx_i18n::use_direction(cx).is_rtl();
                    let keys = {
                        let handles = handles.clone();
                        let focusable = vec![true; handles.len()];
                        move |event: &KeyDownEvent, window: &mut Window, cx: &mut App| {
                            let Some(key) = GridKey::of(&event.keystroke) else {
                                return;
                            };
                            let Some(current) = handles.iter().position(|h| h.is_focused(window))
                            else {
                                return;
                            };
                            if let Some(step) =
                                grid_move(&focusable, current, GRID_COLUMNS, key, rtl)
                            {
                                cx.stop_propagation();
                                if let GridMove::Focus(i) = step {
                                    window.focus(&handles[i], cx);
                                }
                            }
                        }
                    };
                    let cells: Vec<AnyElement> = visible
                        .iter()
                        .enumerate()
                        .map(|(i, option)| {
                            let focus = handles[i].clone().tab_stop(true);
                            let (select, set_open, emoji) =
                                (on_select.clone(), set_open.clone(), option.emoji.clone());
                            let selector = format!("{id}-emoji-{i}");
                            let mut cell = div()
                                .id(ElementId::Name(
                                    format!("{id}-emoji-{}", option.emoji).into(),
                                ))
                                .debug_selector(move || selector)
                                .track_focus(&focus)
                                .relative()
                                .flex()
                                .items_center()
                                .justify_center()
                                .size(px(32.))
                                .rounded(t.radius.inner)
                                .cursor_pointer()
                                .text_size(px(18.))
                                .line_height(t.text.body.size.to_pixels(rem) * t.text.body.leading)
                                .hover(|s| s.bg(t.color.overlay_hover))
                                .role(Role::Button)
                                .aria_label(SharedString::from(format!(
                                    "React with {}",
                                    option.name
                                )))
                                .child(div().inert(true).child(option.emoji.clone()))
                                .on_click(move |_, window, cx| {
                                    if let Some(f) = &select {
                                        f(emoji.clone(), window, cx);
                                    }
                                    set_open(false, cx);
                                });
                            if is_focus_visible(&focus, window) {
                                // Inset, not the shared offset: the cells sit in a tight
                                // 8-across grid, so an outset ring would overlap its
                                // neighbors.
                                cell = cell.child(astryx_components::focus::focus_ring_inset(
                                    &t,
                                    t.radius.inner.to_pixels(rem),
                                    rem,
                                ));
                            }
                            cell.into_any_element()
                        })
                        .collect();
                    div()
                        .id(ElementId::Name(format!("{id}-grid").into()))
                        .grid()
                        .grid_cols(GRID_COLUMNS as u16)
                        .gap(t.spacing.s0_5)
                        .role(Role::Group)
                        .aria_label("Emoji")
                        .on_key_down(keys)
                        .children(cells)
                        .into_any_element()
                };
                let mut panel = div()
                    .flex()
                    .flex_col()
                    .gap(t.spacing.s2)
                    .w(px((GRID_COLUMNS * 34 + 8) as f32));
                theme.refine(panel.style(), "chat-emoji-picker", &[], None);
                panel.style().refine(&style);
                panel.child(input).child(body).into_any_element()
            }
        };
        let on_open = set_open.clone();
        let mut popover = Popover::new(self.id.clone(), self.label.clone())
            .placement(Placement::Below)
            .alignment(LayerAlign::Start)
            .open(open)
            .on_open_change(move |open, _, cx| on_open(open, cx))
            .content_fn(content);
        if let Some(trigger) = self.trigger {
            popover = popover.child(trigger);
        }
        popover
    }
}
