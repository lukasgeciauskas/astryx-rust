//! When a component presents itself the touch way, upstream
//! `utils/touchPresentation.ts`, `utils/presentation.ts`,
//! `hooks/useAdaptivePresentation.ts` and `Layer/presentationHold.ts`.
//!
//! - Touch behaviors key on a coarse primary pointer (a finger):
//!   [`astryx_primitives::coarse_primary_pointer`], which an app can declare with
//!   [`astryx_primitives::modality::set_primary_pointer_coarse`]. A touchscreen laptop
//!   keeps its trackpad's pointer behaviors.
//! - Layout behaviors key on the available size: the active theme's compact definition
//!   (AST-012 FR9), narrower than its `md` point or shorter than 500. A phone in
//!   landscape is compact by its height.
//! - A presentation that needs both asks for both: `Adaptive` is the BottomSheet on a
//!   compact touch screen and the anchored popover everywhere else, so a tablet stays
//!   anchored.
//! - A caller's explicit presentation always wins over a component's touch default:
//!   an explicit `Popover` keeps the fine-pointer surface and input modes on every
//!   device, which an omitted one need not ([`PresentationDefault::Touch`]).
//! - An opening chooses its surface and keeps it until it closes
//!   ([`PresentationHold`]): a phone rotated past the compact width keeps its open
//!   sheet, and a menu never changes shape under the finger that opened it.

use astryx_theme::WidthBreakpoints;
use gpui::{App, Pixels, Size, Window, px};

use super::popover::PopoverPresentation;

/// Upstream `COMPACT_HEIGHT_BELOW`: a viewport shorter than this is compact.
pub const COMPACT_HEIGHT_BELOW: f32 = 500.;

/// Upstream `CompactDefinition`: when the viewport is compact, for a theme's width
/// points (`spec:AST-012` FR9).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CompactDefinition {
    /// Compact below this width: the theme's `md` point.
    pub width_below: f32,
    /// Compact below this height, 500.
    pub height_below: f32,
}

impl CompactDefinition {
    /// Upstream `DEFAULT_COMPACT_DEFINITION`: at the default width points.
    pub const DEFAULT: CompactDefinition = CompactDefinition {
        width_below: WidthBreakpoints::DEFAULT.md,
        height_below: COMPACT_HEIGHT_BELOW,
    };

    /// Upstream `compactDefinition(points)`.
    pub fn from_breakpoints(points: &WidthBreakpoints) -> Self {
        CompactDefinition {
            width_below: points.md,
            height_below: COMPACT_HEIGHT_BELOW,
        }
    }

    /// Whether a viewport of `size` is compact: strictly below either edge.
    pub fn is_compact(&self, size: Size<Pixels>) -> bool {
        size.width < px(self.width_below) || size.height < px(self.height_below)
    }
}

impl Default for CompactDefinition {
    fn default() -> Self {
        CompactDefinition::DEFAULT
    }
}

/// Upstream `useCompactDefinition`: the active theme's compact definition.
pub fn compact_definition(window: &Window, cx: &App) -> CompactDefinition {
    CompactDefinition::from_breakpoints(&astryx_theme::current(window, cx).theme.width_breakpoints)
}

/// Upstream `TouchPresentation`: the two conditions a touch presentation reads.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TouchPresentation {
    /// The primary pointer is coarse (a finger).
    pub is_touch: bool,
    /// The viewport is compact.
    pub is_compact: bool,
}

/// Upstream `useTouchPresentation` and `readTouchPresentation`: the touch
/// presentation now. A render reads it each frame, so it follows the pointer and the
/// size as they change (a tablet docking a trackpad).
pub fn touch_presentation(window: &Window, cx: &mut App) -> TouchPresentation {
    let compact = compact_definition(window, cx);
    TouchPresentation {
        is_touch: astryx_primitives::coarse_primary_pointer(window, cx),
        is_compact: compact.is_compact(window.viewport_size()),
    }
}

/// Upstream `PresentationDefault`: what a component does when the caller omits
/// `presentation`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PresentationDefault {
    /// As an explicit `Popover`.
    #[default]
    Popover,
    /// The popover surface, with the component's touch input modes under a finger.
    Touch,
}

/// The surface a presentation renders.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PresentationSurface {
    /// Anchored to its trigger.
    Popover,
    /// A modal BottomSheet.
    BottomSheet,
}

/// Upstream `ResolvedPresentation`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ResolvedPresentation {
    /// The surface to render.
    pub surface: PresentationSurface,
    /// The component's touch input modes apply: a finger, and `presentation` not
    /// pinned to `Popover`.
    pub touch_modes: bool,
}

/// Upstream `resolvePresentation`: the decision, from a presentation request and a
/// touch presentation.
pub fn resolve_presentation(
    presentation: Option<PopoverPresentation>,
    touch: TouchPresentation,
    when_omitted: PresentationDefault,
) -> ResolvedPresentation {
    let (sheet, pinned) = match presentation {
        Some(PopoverPresentation::BottomSheet) => (true, false),
        Some(PopoverPresentation::Adaptive) => (touch.is_touch && touch.is_compact, false),
        Some(PopoverPresentation::Popover) => (false, true),
        None => (false, when_omitted == PresentationDefault::Popover),
    };
    ResolvedPresentation {
        surface: if sheet {
            PresentationSurface::BottomSheet
        } else {
            PresentationSurface::Popover
        },
        touch_modes: !pinned && touch.is_touch,
    }
}

/// Upstream `usePresentation` and `readPresentation`: the presentation now.
pub fn read_presentation(
    presentation: Option<PopoverPresentation>,
    when_omitted: PresentationDefault,
    window: &Window,
    cx: &mut App,
) -> ResolvedPresentation {
    resolve_presentation(presentation, touch_presentation(window, cx), when_omitted)
}

/// Upstream `useAdaptivePresentation`: `Adaptive` resolved against the active theme's
/// compact-touch condition; a fixed presentation as it is.
pub fn adaptive_presentation(
    presentation: PopoverPresentation,
    window: &Window,
    cx: &mut App,
) -> PopoverPresentation {
    match presentation {
        PopoverPresentation::Adaptive if compact_touch(window, cx) => {
            PopoverPresentation::BottomSheet
        }
        PopoverPresentation::Adaptive => PopoverPresentation::Popover,
        fixed => fixed,
    }
}

/// A compact touch screen (upstream `coarsePointerQuery`): where the adaptive
/// presentations use a sheet, and menus drill into submenus.
pub fn compact_touch(window: &Window, cx: &mut App) -> bool {
    let touch = touch_presentation(window, cx);
    touch.is_touch && touch.is_compact
}

/// Whether `presentation` resolves to a BottomSheet now.
pub(crate) fn presents_as_sheet(
    presentation: PopoverPresentation,
    window: &Window,
    cx: &mut App,
) -> bool {
    adaptive_presentation(presentation, window, cx) == PopoverPresentation::BottomSheet
}

/// Upstream `createPresentationHold`: what an opening chose, held until it is
/// released.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PresentationHold<T: Copy> {
    held: Option<T>,
}

impl<T: Copy> Default for PresentationHold<T> {
    fn default() -> Self {
        PresentationHold { held: None }
    }
}

impl<T: Copy> PresentationHold<T> {
    /// Holds `choice` (a false choice is held too).
    pub fn take(&mut self, choice: T) {
        self.held = Some(choice);
    }

    /// What is held, if anything.
    pub fn current(&self) -> Option<T> {
        self.held
    }

    /// Whether something is held.
    pub fn is_held(&self) -> bool {
        self.held.is_some()
    }

    /// Lets the choice go.
    pub fn release(&mut self) {
        self.held = None;
    }

    /// Upstream `usePresentationHold`, for a surface that renders each frame: while
    /// `open`, the choice made by the frame that opened it (`choose` runs only then);
    /// while closed, the last choice, so a closing surface plays its exit as what it
    /// was, until the next opening chooses again.
    pub fn follow(&mut self, open: bool, was_open: bool, choose: impl FnOnce() -> T) -> T {
        if open && (!was_open || self.held.is_none()) {
            let choice = choose();
            self.held = Some(choice);
            return choice;
        }
        self.held.unwrap_or_else(choose)
    }
}

#[cfg(test)]
#[path = "presentation_tests.rs"]
mod tests;
