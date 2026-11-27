//! Which surface collects a date or time, upstream
//! `utils/inputPresentation.ts` and `DateInput/useCalendarPresentation.ts`
//! (AST-043, AST-111 FR15).

use gpui::Window;
use rustryx_i18n::CalendarSystem;
use rustryx_primitives::dev_warn_once;

/// Upstream `presentation` for DateInput and DateTimeInput: every value
/// opens a picker (AST-043 FR1).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum PickerPresentation {
    /// The typed field and a popover, on every pointer.
    Popover,
    /// The bottom sheet, on every pointer.
    BottomSheet,
    /// The platform's own picker, on every pointer.
    Native,
    /// The popover on a fine pointer, the bottom sheet on a coarse one.
    AdaptiveBottomSheet,
    /// The popover on a fine pointer, the platform picker on a coarse one.
    #[default]
    AdaptiveNative,
}

/// Upstream `presentation` for TimeInput, which adds the typed field alone.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum InputPresentation {
    /// The typed field on every pointer.
    TextInput,
    Popover,
    BottomSheet,
    Native,
    AdaptiveBottomSheet,
    #[default]
    AdaptiveNative,
}

impl From<PickerPresentation> for InputPresentation {
    fn from(p: PickerPresentation) -> Self {
        match p {
            PickerPresentation::Popover => InputPresentation::Popover,
            PickerPresentation::BottomSheet => InputPresentation::BottomSheet,
            PickerPresentation::Native => InputPresentation::Native,
            PickerPresentation::AdaptiveBottomSheet => InputPresentation::AdaptiveBottomSheet,
            PickerPresentation::AdaptiveNative => InputPresentation::AdaptiveNative,
        }
    }
}

/// Upstream's deprecated `nativePicker`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum LegacyNativePicker {
    /// Native on a coarse pointer: `AdaptiveNative`.
    Touch,
    /// Native everywhere, with the released fallbacks: `Native`.
    Always,
    /// Astryx's own pickers: `AdaptiveBottomSheet` (TimeInput's typed field).
    Never,
}

/// The surface a presentation renders now.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum Surface {
    TextInput,
    Desktop,
    Sheet,
    Native,
}

/// A coarse primary pointer: a finger (upstream `(pointer: coarse)`), read
/// as the last pointer input having been touch; a key press keeps it. There
/// is no width bound (FR4).
pub(crate) fn coarse_pointer(window: &Window, cx: &mut gpui::App) -> bool {
    rustryx_primitives::coarse_primary_pointer(window, cx)
}

/// The effective policy of a date picker (`effectivePickerPresentation`):
/// `presentation` wins over the deprecated `nativePicker` (AST-043 FR4).
pub(crate) fn effective_picker(
    presentation: Option<PickerPresentation>,
    native_picker: Option<LegacyNativePicker>,
    source: &str,
) -> PickerPresentation {
    if native_picker.is_some() {
        dev_warn_once(&format!("native-picker-deprecated:{source}"), || {
            format!(
                "{source}: `native_picker` is deprecated; use `presentation` instead (Touch → AdaptiveNative, Always → Native, Never → AdaptiveBottomSheet). `presentation` wins when both are set."
            )
        });
    }
    if let Some(p) = presentation {
        return p;
    }
    match native_picker {
        Some(LegacyNativePicker::Touch) | None => PickerPresentation::AdaptiveNative,
        Some(LegacyNativePicker::Always) => PickerPresentation::Native,
        Some(LegacyNativePicker::Never) => PickerPresentation::AdaptiveBottomSheet,
    }
}

/// The effective policy of TimeInput (`effectiveInputPresentation`): its
/// released `never` is the typed field.
pub(crate) fn effective_input(
    presentation: Option<InputPresentation>,
    native_picker: Option<LegacyNativePicker>,
    source: &str,
) -> InputPresentation {
    if let Some(p) = presentation {
        if native_picker.is_some() {
            effective_picker(None, native_picker, source);
        }
        return p;
    }
    match native_picker {
        Some(LegacyNativePicker::Never) => {
            effective_picker(None, native_picker, source);
            InputPresentation::TextInput
        }
        other => effective_picker(None, other, source).into(),
    }
}

/// The policy a date picker follows in its calendar
/// (`calendarPickerPresentation`, AST-111 FR15): the native control is
/// Gregorian, so in another calendar the policies that use it use the
/// popover on a fine pointer and the sheet on a coarse one; an explicit
/// `Native` also warns once.
pub(crate) fn calendar_picker(
    presentation: PickerPresentation,
    calendar: CalendarSystem,
    source: &str,
) -> PickerPresentation {
    if calendar.is_gregorian() {
        return presentation;
    }
    match presentation {
        PickerPresentation::Native => {
            dev_warn_once(
                &format!(
                    "presentation-native-calendar:{source}:{}",
                    calendar.identifier()
                ),
                || {
                    format!(
                        "{source}: presentation Native shows the Astryx picker in the \"{}\" calendar, because the native date control shows only Gregorian dates: the popover on a fine pointer and the bottom sheet on a coarse pointer.",
                        calendar.identifier()
                    )
                },
            );
            PickerPresentation::AdaptiveBottomSheet
        }
        PickerPresentation::AdaptiveNative => PickerPresentation::AdaptiveBottomSheet,
        other => other,
    }
}

/// The surface a picker policy renders for the pointer
/// (`resolvePickerPresentation`).
pub(crate) fn resolve_picker(presentation: PickerPresentation, coarse: bool) -> Surface {
    match presentation {
        PickerPresentation::Popover => Surface::Desktop,
        PickerPresentation::BottomSheet => Surface::Sheet,
        PickerPresentation::Native => Surface::Native,
        PickerPresentation::AdaptiveBottomSheet if coarse => Surface::Sheet,
        PickerPresentation::AdaptiveBottomSheet => Surface::Desktop,
        PickerPresentation::AdaptiveNative if coarse => Surface::Native,
        PickerPresentation::AdaptiveNative => Surface::Desktop,
    }
}

/// The surface any policy renders (`resolveInputPresentation`).
pub(crate) fn resolve_input(presentation: InputPresentation, coarse: bool) -> Surface {
    let picker = match presentation {
        InputPresentation::TextInput => return Surface::TextInput,
        InputPresentation::Popover => PickerPresentation::Popover,
        InputPresentation::BottomSheet => PickerPresentation::BottomSheet,
        InputPresentation::Native => PickerPresentation::Native,
        InputPresentation::AdaptiveBottomSheet => PickerPresentation::AdaptiveBottomSheet,
        InputPresentation::AdaptiveNative => PickerPresentation::AdaptiveNative,
    };
    resolve_picker(picker, coarse)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ast043_fr1_values_resolve_by_pointer() {
        use PickerPresentation::*;
        assert_eq!(resolve_picker(Popover, true), Surface::Desktop);
        assert_eq!(resolve_picker(BottomSheet, false), Surface::Sheet);
        assert_eq!(resolve_picker(Native, false), Surface::Native);
        assert_eq!(resolve_picker(AdaptiveBottomSheet, false), Surface::Desktop);
        assert_eq!(resolve_picker(AdaptiveBottomSheet, true), Surface::Sheet);
        assert_eq!(resolve_picker(AdaptiveNative, false), Surface::Desktop);
        assert_eq!(resolve_picker(AdaptiveNative, true), Surface::Native);
        assert_eq!(
            resolve_input(InputPresentation::TextInput, true),
            Surface::TextInput
        );
    }

    #[test]
    fn ast043_fr3_fr4_the_deprecated_prop_maps_and_loses() {
        use LegacyNativePicker::*;
        assert_eq!(
            effective_picker(None, None, "t"),
            PickerPresentation::AdaptiveNative
        );
        assert_eq!(
            effective_picker(None, Some(Touch), "t"),
            PickerPresentation::AdaptiveNative
        );
        assert_eq!(
            effective_picker(None, Some(Always), "t"),
            PickerPresentation::Native
        );
        assert_eq!(
            effective_picker(None, Some(Never), "t"),
            PickerPresentation::AdaptiveBottomSheet
        );
        assert_eq!(
            effective_picker(Some(PickerPresentation::Popover), Some(Always), "t"),
            PickerPresentation::Popover
        );
        assert_eq!(
            effective_input(None, Some(Never), "t"),
            InputPresentation::TextInput
        );
    }

    #[test]
    fn ast111_fr15_another_calendar_never_shows_the_native_control() {
        use PickerPresentation::*;
        assert_eq!(
            calendar_picker(Native, CalendarSystem::Persian, "t"),
            AdaptiveBottomSheet
        );
        assert_eq!(
            calendar_picker(AdaptiveNative, CalendarSystem::Hebrew, "t"),
            AdaptiveBottomSheet
        );
        assert_eq!(
            calendar_picker(Native, CalendarSystem::Iso8601, "t"),
            Native
        );
        assert_eq!(
            calendar_picker(Popover, CalendarSystem::Persian, "t"),
            Popover
        );
    }
}
