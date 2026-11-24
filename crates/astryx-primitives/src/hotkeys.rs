//! Global keyboard shortcuts, upstream `useHotkeys`.
//!
//! [`hotkeys`] wraps an element and, while it is drawn, sees every key press
//! in its window after GPUI has dispatched it, focused or not, as upstream's
//! `window` `keydown` listener does. A key that a focused element consumed,
//! or that a key binding turned into an action, is not a shortcut: that is
//! upstream's `defaultPrevented`.
//!
//! Keys are `+`-separated combos: `"mod+k"`, `"?"`, `"escape"`. `mod` is ⌘
//! on macOS and Ctrl elsewhere. Name shifted punctuation by the character it
//! types (`"?"`, not `"shift+/"`).

use std::cell::RefCell;
use std::panic::Location;
use std::rc::Rc;

use gpui::{
    AnyElement, App, Bounds, Element, ElementId, GlobalElementId, InspectorElementId, IntoElement,
    Keystroke, KeystrokeEvent, LayoutId, Pixels, SharedString, Subscription, Window,
};

use crate::modality::TYPING_TARGET;

type Handler = Rc<dyn Fn(&Keystroke, &mut Window, &mut App)>;

/// One shortcut, upstream `Hotkey`.
#[derive(Clone)]
pub struct Hotkey {
    keys: SharedString,
    on_press: Handler,
    allow_in_inputs: bool,
    disabled: bool,
}

impl Hotkey {
    /// A shortcut for `keys` (`"mod+k"`) that calls `on_press`.
    pub fn new(
        keys: impl Into<SharedString>,
        on_press: impl Fn(&Keystroke, &mut Window, &mut App) + 'static,
    ) -> Self {
        Self {
            keys: keys.into(),
            on_press: Rc::new(on_press),
            allow_in_inputs: false,
            disabled: false,
        }
    }

    /// Also fires while typing in a text input, upstream `allowInInputs`.
    pub fn allow_in_inputs(mut self, allow: bool) -> Self {
        self.allow_in_inputs = allow;
        self
    }

    /// Suspends the shortcut without removing it, upstream `isDisabled`.
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }
}

const KEY_ALIASES: &[(&str, &str)] = &[
    ("esc", "escape"),
    ("space", " "),
    ("up", "arrowup"),
    ("down", "arrowdown"),
    ("left", "arrowleft"),
    ("right", "arrowright"),
    ("return", "enter"),
    ("plus", "+"),
];

/// The DOM `KeyboardEvent.key` name for a GPUI key name, lowercased.
fn dom_key(gpui_key: &str) -> String {
    match gpui_key {
        "up" => "arrowup".into(),
        "down" => "arrowdown".into(),
        "left" => "arrowleft".into(),
        "right" => "arrowright".into(),
        "space" => " ".into(),
        key => key.to_lowercase(),
    }
}

/// Whether `keys` matches a keystroke, upstream `matchesCombo`.
pub fn matches_combo(keys: &str, keystroke: &Keystroke) -> bool {
    let lower = keys.to_lowercase();
    // `+` is a separator, so a trailing `+` alone is the plus key.
    let parts: Vec<&str> = if lower == "+" {
        vec!["+"]
    } else {
        lower
            .split('+')
            .map(str::trim)
            .filter(|part| !part.is_empty())
            .collect()
    };
    let Some((&key, mods)) = parts.split_last() else {
        return false;
    };
    let apple = cfg!(target_os = "macos");
    let wants_mod = mods.contains(&"mod");
    let wants_ctrl = mods.contains(&"ctrl") || (wants_mod && !apple);
    let wants_meta = mods.contains(&"meta") || (wants_mod && apple);
    let wants_alt = mods.contains(&"alt");
    let wants_shift = mods.contains(&"shift");

    let modifiers = &keystroke.modifiers;
    if modifiers.control != wants_ctrl
        || modifiers.platform != wants_meta
        || modifiers.alt != wants_alt
        || (wants_shift && !modifiers.shift)
    {
        return false;
    }

    let expected = KEY_ALIASES
        .iter()
        .find(|(alias, _)| *alias == key)
        .map_or(key, |(_, name)| *name);
    // `KeyboardEvent.key` is the typed character when there is one, and the
    // key's name otherwise (Ctrl+K types nothing: its key is still "k").
    let typed = keystroke
        .key_char
        .as_deref()
        .filter(|c| !c.chars().any(char::is_control) && !c.is_empty())
        .map(str::to_lowercase);
    match typed {
        Some(typed) => typed == expected,
        None => dom_key(&keystroke.key) == expected,
    }
}

/// Wraps `child` so `shortcuts` fire while it is drawn, upstream
/// `useHotkeys(hotkeys)`. Put it at the root of a window's view for
/// window-wide shortcuts.
#[track_caller]
pub fn hotkeys(shortcuts: impl IntoIterator<Item = Hotkey>, child: impl IntoElement) -> Hotkeys {
    Hotkeys {
        shortcuts: shortcuts.into_iter().collect(),
        child: child.into_any_element(),
        source: Location::caller(),
    }
}

/// The element [`hotkeys`] returns. It takes its child's box.
pub struct Hotkeys {
    shortcuts: Rc<[Hotkey]>,
    child: AnyElement,
    source: &'static Location<'static>,
}

/// Lives in element state while the element is drawn: dropping it ends the
/// keystroke subscription.
struct Registration {
    shortcuts: Rc<RefCell<Rc<[Hotkey]>>>,
    _subscription: Subscription,
}

fn on_keystroke(shortcuts: &[Hotkey], event: &KeystrokeEvent, window: &mut Window, cx: &mut App) {
    if event.action.is_some() {
        return;
    }
    let typing = event
        .context_stack
        .iter()
        .any(|context| context.contains(TYPING_TARGET));
    for shortcut in shortcuts {
        if shortcut.disabled || (typing && !shortcut.allow_in_inputs) {
            continue;
        }
        if matches_combo(&shortcut.keys, &event.keystroke) {
            (shortcut.on_press)(&event.keystroke, window, cx);
            return;
        }
    }
}

impl Element for Hotkeys {
    type RequestLayoutState = ();
    type PrepaintState = ();

    fn id(&self) -> Option<ElementId> {
        Some(ElementId::CodeLocation(*self.source))
    }

    fn source_location(&self) -> Option<&'static Location<'static>> {
        Some(self.source)
    }

    fn request_layout(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, ()) {
        (self.child.request_layout(window, cx), ())
    }

    fn prepaint(
        &mut self,
        id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        _bounds: Bounds<Pixels>,
        _request_layout: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) {
        let id = id.expect("hotkeys have an element id");
        window.with_element_state::<Registration, _>(id, |registration, window| {
            let registration = registration.unwrap_or_else(|| {
                let shortcuts = Rc::new(RefCell::new(self.shortcuts.clone()));
                let handle = window.window_handle();
                let subscription = cx.observe_keystrokes({
                    let shortcuts = shortcuts.clone();
                    move |event, window, cx| {
                        if window.window_handle() == handle {
                            let current = shortcuts.borrow().clone();
                            on_keystroke(&current, event, window, cx);
                        }
                    }
                });
                Registration {
                    shortcuts,
                    _subscription: subscription,
                }
            });
            *registration.shortcuts.borrow_mut() = self.shortcuts.clone();
            ((), registration)
        });
        self.child.prepaint(window, cx);
    }

    fn paint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        _bounds: Bounds<Pixels>,
        _request_layout: &mut (),
        _prepaint: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) {
        self.child.paint(window, cx);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stroke(source: &'static str, key_char: Option<&str>) -> Keystroke {
        let mut keystroke = Keystroke::parse(source).unwrap();
        keystroke.key_char = key_char.map(Into::into);
        keystroke
    }

    #[test]
    fn mod_is_ctrl_off_macos() {
        let ctrl_k = stroke("ctrl-k", None);
        assert_eq!(matches_combo("mod+k", &ctrl_k), !cfg!(target_os = "macos"));
        assert!(matches_combo("ctrl+k", &ctrl_k));
        assert!(!matches_combo("k", &ctrl_k));
        assert!(!matches_combo("mod+shift+k", &ctrl_k));
    }

    #[test]
    fn shifted_punctuation_matches_the_typed_character() {
        let question = stroke("shift-/", Some("?"));
        assert!(matches_combo("?", &question));
        assert!(matches_combo("shift+?", &question));
        assert!(!matches_combo("/", &question));
    }

    #[test]
    fn aliases_and_named_keys() {
        assert!(matches_combo("esc", &stroke("escape", None)));
        assert!(matches_combo("Escape", &stroke("escape", None)));
        assert!(matches_combo("up", &stroke("up", None)));
        assert!(matches_combo("space", &stroke("space", Some(" "))));
        assert!(matches_combo("enter", &stroke("enter", None)));
    }

    #[test]
    fn letters_ignore_case_and_extra_shift() {
        assert!(matches_combo("a", &stroke("a", Some("a"))));
        assert!(matches_combo("a", &stroke("shift-a", Some("A"))));
        assert!(!matches_combo("shift+a", &stroke("a", Some("a"))));
        assert!(!matches_combo("alt+a", &stroke("a", Some("a"))));
    }
}
