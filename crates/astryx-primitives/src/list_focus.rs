//! Arrow-key navigation in a linear list, upstream `useListFocus`, with the
//! roving tab stop of composites like Toolbar, RadioList and TabList.
//!
//! [`ListKeys`] is the key logic. [`ListFocus`] holds one focus handle per
//! item between frames and supplies the container's key-down listener:
//!
//! ```ignore
//! let list = ListFocus::new(&id, items.len(), ListKeys::horizontal(), window, cx)
//!     .disabled(disabled_flags)
//!     .rtl(dir.is_rtl());
//! let tab_stop = list.tab_stop(selected, window);
//! div().on_key_down(list.key_down_listener()).children(items.iter().enumerate().map(|(i, item)| {
//!     div().track_focus(&list.item_handle(i, tab_stop)) // ...
//! }))
//! ```

use std::rc::Rc;

use gpui::{App, ElementId, FocusHandle, KeyDownEvent, Window};

/// Upstream `orientation`: which arrows move.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ListOrientation {
    /// Left and Right, mirrored in right-to-left text.
    Horizontal,
    /// Up and Down.
    #[default]
    Vertical,
    /// All four arrows.
    Both,
}

/// The key rules of a list, upstream `useListFocus` options.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ListKeys {
    pub orientation: ListOrientation,
    /// Upstream `wrap`: arrows wrap around at the ends.
    pub wrap: bool,
    /// Upstream `hasHomeEnd`: Home and End jump to the first and last items.
    pub home_end: bool,
    /// Upstream `reachesDisabledItems`: arrows stop on disabled items too.
    pub reach_disabled: bool,
    /// Right-to-left text: Left moves forward.
    pub rtl: bool,
}

impl Default for ListKeys {
    fn default() -> Self {
        ListKeys {
            orientation: ListOrientation::Vertical,
            wrap: true,
            home_end: true,
            reach_disabled: false,
            rtl: false,
        }
    }
}

/// A step through the items.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Step {
    Next,
    Prev,
    First,
    Last,
}

impl ListKeys {
    pub fn horizontal() -> Self {
        ListKeys {
            orientation: ListOrientation::Horizontal,
            ..Default::default()
        }
    }

    pub fn vertical() -> Self {
        ListKeys::default()
    }

    pub fn both() -> Self {
        ListKeys {
            orientation: ListOrientation::Both,
            ..Default::default()
        }
    }

    pub fn wrap(mut self, wrap: bool) -> Self {
        self.wrap = wrap;
        self
    }

    pub fn home_end(mut self, home_end: bool) -> Self {
        self.home_end = home_end;
        self
    }

    pub fn reach_disabled(mut self, reach: bool) -> Self {
        self.reach_disabled = reach;
        self
    }

    pub fn rtl(mut self, rtl: bool) -> Self {
        self.rtl = rtl;
        self
    }

    fn step(&self, key: &str) -> Option<Step> {
        let horizontal = matches!(
            self.orientation,
            ListOrientation::Horizontal | ListOrientation::Both
        );
        let vertical = matches!(
            self.orientation,
            ListOrientation::Vertical | ListOrientation::Both
        );
        let (forward, back) = if self.rtl {
            ("left", "right")
        } else {
            ("right", "left")
        };
        Some(match key {
            k if horizontal && k == forward => Step::Next,
            k if horizontal && k == back => Step::Prev,
            "down" if vertical => Step::Next,
            "up" if vertical => Step::Prev,
            "home" if self.home_end => Step::First,
            "end" if self.home_end => Step::Last,
            _ => return None,
        })
    }

    /// Whether `key` (a GPUI key name) is one of this list's keys. A list
    /// consumes its keys even when there is nowhere to move.
    pub fn handles(&self, key: &str) -> bool {
        self.step(key).is_some()
    }

    /// The item `key` moves to from `current` (`None` when focus is on the
    /// list itself), or `None` when nothing enabled lies that way.
    pub fn target(
        &self,
        key: &str,
        current: Option<usize>,
        count: usize,
        disabled: impl Fn(usize) -> bool,
    ) -> Option<usize> {
        let skipped = |i: usize| !self.reach_disabled && disabled(i);
        let count = count as isize;
        let current = current.map(|c| c as isize);
        match self.step(key)? {
            Step::Next => find(count, current.map_or(0, |c| c + 1), 1, self.wrap, skipped),
            Step::Prev => find(
                count,
                current.map_or(count - 1, |c| c - 1),
                -1,
                self.wrap,
                skipped,
            ),
            Step::First => find(count, 0, 1, false, skipped),
            Step::Last => find(count, count - 1, -1, false, skipped),
        }
    }
}

/// Upstream `findItemIndex`: from `start`, stepping, the first item not
/// skipped.
fn find(
    count: isize,
    start: isize,
    step: isize,
    wrap: bool,
    skipped: impl Fn(usize) -> bool,
) -> Option<usize> {
    let mut index = start;
    for _ in 0..count {
        if !(0..count).contains(&index) {
            if !wrap {
                return None;
            }
            index = index.rem_euclid(count);
        }
        if !skipped(index as usize) {
            return Some(index as usize);
        }
        index += step;
    }
    None
}

/// Remembered between frames: the item handles and the last item focused.
struct ListFocusState {
    handles: Vec<FocusHandle>,
    last_focused: Option<usize>,
}

type EscapeFn = Rc<dyn Fn(&mut Window, &mut App)>;
type MoveFn = Rc<dyn Fn(usize, &mut Window, &mut App)>;

/// A list's item focus, upstream `useListFocus`. Build it each render.
#[derive(Clone)]
pub struct ListFocus {
    handles: Rc<[FocusHandle]>,
    last_focused: Option<usize>,
    keys: ListKeys,
    disabled: Rc<[bool]>,
    on_escape: Option<EscapeFn>,
    on_move: Option<MoveFn>,
}

impl ListFocus {
    /// The focus of the list with id `id` and `count` items.
    pub fn new(
        id: &ElementId,
        count: usize,
        keys: ListKeys,
        window: &mut Window,
        cx: &mut App,
    ) -> Self {
        let state = window.use_keyed_state(
            ElementId::Name(format!("{id:?}-list-focus").into()),
            cx,
            |_, _| ListFocusState {
                handles: Vec::new(),
                last_focused: None,
            },
        );
        let (handles, last_focused) = state.update(cx, |state, cx| {
            while state.handles.len() < count {
                state.handles.push(cx.focus_handle());
            }
            let handles: Rc<[FocusHandle]> = state.handles[..count].into();
            if let Some(focused) = handles.iter().position(|h| h.is_focused(window)) {
                state.last_focused = Some(focused);
            }
            (handles, state.last_focused.filter(|&i| i < count))
        });
        ListFocus {
            handles,
            last_focused,
            keys,
            disabled: Rc::from(vec![false; count]),
            on_escape: None,
            on_move: None,
        }
    }

    /// Which items are disabled; arrows skip them unless the keys reach them.
    pub fn disabled(mut self, disabled: impl IntoIterator<Item = bool>) -> Self {
        let mut flags: Vec<bool> = disabled.into_iter().collect();
        flags.resize(self.handles.len(), false);
        self.disabled = flags.into();
        self
    }

    /// Right-to-left text: horizontal arrows mirror.
    pub fn rtl(mut self, rtl: bool) -> Self {
        self.keys.rtl = rtl;
        self
    }

    /// Upstream `onEscape`: Escape calls it and stops there; without it,
    /// Escape reaches the surrounding layer.
    pub fn on_escape(mut self, f: impl Fn(&mut Window, &mut App) + 'static) -> Self {
        self.on_escape = Some(Rc::new(f));
        self
    }

    /// Called with the item each arrow key moves to, after focusing it, so
    /// a list can follow focus (radios select as they move).
    pub fn on_move(mut self, f: impl Fn(usize, &mut Window, &mut App) + 'static) -> Self {
        self.on_move = Some(Rc::new(f));
        self
    }

    pub fn len(&self) -> usize {
        self.handles.len()
    }

    pub fn is_empty(&self) -> bool {
        self.handles.is_empty()
    }

    /// The item that has focus.
    pub fn focused(&self, window: &Window) -> Option<usize> {
        self.handles.iter().position(|h| h.is_focused(window))
    }

    fn enabled(&self, index: usize) -> bool {
        !self.disabled.get(index).copied().unwrap_or(true)
    }

    /// The roving tab stop (upstream `hasRovingTabIndex`): the focused
    /// item, else the last one focused, else `preferred` (a selection),
    /// else the first enabled item.
    pub fn tab_stop(&self, preferred: Option<usize>, window: &Window) -> Option<usize> {
        self.focused(window)
            .or(self.last_focused.filter(|&i| self.enabled(i)))
            .or(preferred.filter(|&i| self.enabled(i)))
            .or_else(|| (0..self.len()).find(|&i| self.enabled(i)))
    }

    /// Item `index`'s focus handle, in the tab order only when it is the
    /// roving `tab_stop`.
    pub fn item_handle(&self, index: usize, tab_stop: Option<usize>) -> FocusHandle {
        self.handles[index]
            .clone()
            .tab_stop(tab_stop == Some(index))
    }

    /// Item `index`'s focus handle, every item a tab stop (lists that are
    /// not roving composites).
    pub fn handle(&self, index: usize) -> &FocusHandle {
        &self.handles[index]
    }

    /// Focuses item `index`, upstream `focusItem`.
    pub fn focus(&self, index: usize, window: &mut Window, cx: &mut App) {
        if let Some(handle) = self.handles.get(index) {
            handle.focus(window, cx);
        }
    }

    /// The container's key-down listener: arrows, Home and End move focus;
    /// Escape calls `on_escape`.
    pub fn key_down_listener(&self) -> impl Fn(&KeyDownEvent, &mut Window, &mut App) + 'static {
        let list = self.clone();
        move |event, window, cx| {
            let keystroke = &event.keystroke;
            let m = &keystroke.modifiers;
            if m.control || m.alt || m.platform {
                return;
            }
            let key = keystroke.key.as_str();
            if key == "escape" {
                if let Some(f) = &list.on_escape {
                    f(window, cx);
                    cx.stop_propagation();
                }
                return;
            }
            if !list.keys.handles(key) {
                return;
            }
            let current = list.focused(window);
            let disabled = |i: usize| list.disabled.get(i).copied().unwrap_or(false);
            if let Some(target) = list.keys.target(key, current, list.len(), disabled) {
                list.focus(target, window, cx);
                if let Some(f) = &list.on_move {
                    f(target, window, cx);
                }
            }
            cx.stop_propagation();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arrows_step_and_wrap() {
        let keys = ListKeys::horizontal();
        let none = |_| false;
        assert_eq!(keys.target("right", Some(0), 3, none), Some(1));
        assert_eq!(keys.target("right", Some(2), 3, none), Some(0));
        assert_eq!(keys.target("left", Some(0), 3, none), Some(2));
        assert_eq!(
            keys.target("down", Some(0), 3, none),
            None,
            "not this orientation"
        );
        assert_eq!(keys.wrap(false).target("right", Some(2), 3, none), None);
    }

    #[test]
    fn from_the_list_itself_arrows_reach_the_ends() {
        let keys = ListKeys::vertical();
        assert_eq!(keys.target("down", None, 3, |_| false), Some(0));
        assert_eq!(keys.target("up", None, 3, |_| false), Some(2));
    }

    #[test]
    fn disabled_items_are_skipped_unless_reached() {
        let keys = ListKeys::vertical();
        let disabled = |i| i == 1;
        assert_eq!(keys.target("down", Some(0), 3, disabled), Some(2));
        assert_eq!(keys.target("home", Some(2), 3, |i| i == 0), Some(1));
        assert_eq!(keys.target("end", Some(0), 3, |i| i == 2), Some(1));
        assert_eq!(
            keys.reach_disabled(true)
                .target("down", Some(0), 3, disabled),
            Some(1)
        );
    }

    #[test]
    fn right_to_left_mirrors_horizontal_arrows() {
        let keys = ListKeys::horizontal().rtl(true);
        assert_eq!(keys.target("left", Some(0), 3, |_| false), Some(1));
        assert_eq!(keys.target("right", Some(0), 3, |_| false), Some(2));
    }

    #[test]
    fn home_and_end_never_wrap_and_can_be_off() {
        let keys = ListKeys::both();
        assert_eq!(keys.target("home", Some(2), 3, |_| false), Some(0));
        assert_eq!(keys.target("end", Some(0), 3, |_| false), Some(2));
        assert!(!keys.home_end(false).handles("home"));
    }
}
