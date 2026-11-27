//! Presentation tests, after upstream `utils/presentation.test.ts`,
//! `utils/touchPresentation.test.ts` and `Layer/presentationHold.test.tsx`.

use gpui::{Context, IntoElement, Render, TestAppContext, Window, div, px, size};

use super::*;

/// A device: whether its primary pointer is coarse, and its viewport.
#[derive(Clone, Copy, Debug)]
struct Device {
    coarse: bool,
    width: f32,
    height: f32,
}

const IPHONE: Device = Device {
    coarse: true,
    width: 393.,
    height: 852.,
};
const IPAD: Device = Device {
    coarse: true,
    width: 820.,
    height: 1180.,
};
const ANDROID: Device = Device {
    coarse: true,
    width: 412.,
    height: 915.,
};
const DESKTOP: Device = Device {
    coarse: false,
    width: 1280.,
    height: 800.,
};
const MAC_SAFARI: Device = Device {
    coarse: false,
    width: 1440.,
    height: 900.,
};
/// A narrow window under a fine pointer: compact by size, never by touch.
const FINE_NARROW: Device = Device {
    coarse: false,
    width: 600.,
    height: 800.,
};
/// A phone in landscape: compact by its height alone.
const PHONE_LANDSCAPE: Device = Device {
    coarse: true,
    width: 852.,
    height: 393.,
};
/// The compact edges at the default md point (768 wide, 500 high).
const COMPACT_EDGES: [Device; 4] = [
    Device {
        coarse: true,
        width: 767.,
        height: 800.,
    },
    Device {
        coarse: true,
        width: 768.,
        height: 800.,
    },
    Device {
        coarse: true,
        width: 1024.,
        height: 499.,
    },
    Device {
        coarse: true,
        width: 1024.,
        height: 500.,
    },
];

fn touch(device: Device, compact: CompactDefinition) -> TouchPresentation {
    TouchPresentation {
        is_touch: device.coarse,
        is_compact: compact.is_compact(size(px(device.width), px(device.height))),
    }
}

fn resolve(
    device: Device,
    presentation: Option<PopoverPresentation>,
    when_omitted: PresentationDefault,
) -> ResolvedPresentation {
    resolve_presentation(
        presentation,
        touch(device, CompactDefinition::DEFAULT),
        when_omitted,
    )
}

const POPOVER: ResolvedPresentation = ResolvedPresentation {
    surface: PresentationSurface::Popover,
    touch_modes: false,
};

#[test]
fn pins_the_fine_surface_and_input_modes_with_an_explicit_popover_even_on_an_iphone() {
    for when_omitted in [PresentationDefault::Popover, PresentationDefault::Touch] {
        assert_eq!(
            resolve(IPHONE, Some(PopoverPresentation::Popover), when_omitted),
            POPOVER
        );
    }
}

#[test]
fn tells_an_omitted_value_from_an_explicit_popover_the_touch_default_applies_only_when_omitted() {
    assert_eq!(
        resolve(IPHONE, None, PresentationDefault::Touch),
        ResolvedPresentation {
            surface: PresentationSurface::Popover,
            touch_modes: true
        }
    );
    // Today's components: omitted is popover.
    assert_eq!(
        resolve(IPHONE, None, PresentationDefault::Popover),
        resolve(
            IPHONE,
            Some(PopoverPresentation::Popover),
            PresentationDefault::Popover
        )
    );
}

#[test]
fn turns_touch_modes_on_under_a_finger_only_the_same_on_every_touch_device() {
    assert!(resolve(ANDROID, None, PresentationDefault::Touch).touch_modes);
    assert!(resolve(IPHONE, None, PresentationDefault::Touch).touch_modes);
    for desk in [DESKTOP, MAC_SAFARI] {
        assert!(!resolve(desk, None, PresentationDefault::Touch).touch_modes);
    }
}

#[test]
fn keeps_the_sheet_meaning_of_adaptive_and_bottom_sheet_with_touch_modes_under_a_finger() {
    let adaptive = Some(PopoverPresentation::Adaptive);
    let omitted = PresentationDefault::Popover;
    assert_eq!(
        resolve(IPHONE, adaptive, omitted),
        ResolvedPresentation {
            surface: PresentationSurface::BottomSheet,
            touch_modes: true
        }
    );
    assert_eq!(
        resolve(IPAD, adaptive, omitted),
        ResolvedPresentation {
            surface: PresentationSurface::Popover,
            touch_modes: true
        }
    );
    assert_eq!(resolve(DESKTOP, adaptive, omitted), POPOVER);
    assert_eq!(
        resolve(DESKTOP, Some(PopoverPresentation::BottomSheet), omitted),
        ResolvedPresentation {
            surface: PresentationSurface::BottomSheet,
            touch_modes: false
        }
    );
}

#[test]
fn answers_adaptive_with_the_sheet_only_on_a_compact_touch_screen_ast094_fr13_ast012_fr9() {
    let adaptive = Some(PopoverPresentation::Adaptive);
    for (device, sheet) in [
        (FINE_NARROW, false),
        (PHONE_LANDSCAPE, true),
        (COMPACT_EDGES[0], true),
        (COMPACT_EDGES[1], false),
        (COMPACT_EDGES[2], true),
        (COMPACT_EDGES[3], false),
    ] {
        let surface = resolve(device, adaptive, PresentationDefault::Popover).surface;
        assert_eq!(
            surface == PresentationSurface::BottomSheet,
            sheet,
            "{device:?}"
        );
    }
}

#[test]
fn is_the_theme_compact_definition_width_below_md_or_height_below_500_ast012_fr9() {
    let compact = CompactDefinition::DEFAULT;
    assert_eq!(compact.width_below, 768.);
    assert_eq!(compact.height_below, 500.);
    assert!(compact.is_compact(size(px(767.), px(900.))));
    assert!(!compact.is_compact(size(px(768.), px(900.))));
    assert!(compact.is_compact(size(px(1200.), px(499.))));
    assert!(!compact.is_compact(size(px(1200.), px(500.))));
}

#[test]
fn reads_the_compact_definition_it_is_given_so_a_theme_that_moves_md_moves_the_sheet_ast012_fr9() {
    let wide_md = CompactDefinition::from_breakpoints(&rustryx_theme::WidthBreakpoints {
        md: 1000.,
        lg: 1100.,
        ..rustryx_theme::WidthBreakpoints::DEFAULT
    });
    let device = Device {
        coarse: true,
        width: 900.,
        height: 1024.,
    };
    let adaptive = Some(PopoverPresentation::Adaptive);
    // Under the default md point (768) this screen is not compact.
    let plain = resolve_presentation(
        adaptive,
        touch(device, CompactDefinition::DEFAULT),
        PresentationDefault::Popover,
    );
    assert_eq!(plain.surface, PresentationSurface::Popover);
    // Under a theme whose md is 1000, the same screen is compact.
    let themed = resolve_presentation(
        adaptive,
        touch(device, wide_md),
        PresentationDefault::Popover,
    );
    assert_eq!(
        themed,
        ResolvedPresentation {
            surface: PresentationSurface::BottomSheet,
            touch_modes: true
        }
    );
}

struct Empty;

impl Render for Empty {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
    }
}

#[gpui::test]
fn keys_touch_on_the_primary_pointer_and_compact_on_the_size_and_follows_both_as_they_change(
    cx: &mut TestAppContext,
) {
    cx.update(rustryx_theme::init);
    let (_, cx) = cx.add_window_view(|_, _| Empty);
    // A desktop window with a fine pointer.
    cx.update(|window, cx| {
        assert_eq!(touch_presentation(window, cx), TouchPresentation::default());
        assert!(!compact_touch(window, cx));
    });
    // A phone: the app declares a finger and the window is phone-sized.
    cx.update(|_, cx| rustryx_primitives::modality::set_primary_pointer_coarse(Some(true), cx));
    cx.simulate_resize(size(px(393.), px(852.)));
    cx.update(|window, cx| {
        assert_eq!(
            touch_presentation(window, cx),
            TouchPresentation {
                is_touch: true,
                is_compact: true
            }
        );
        assert_eq!(
            adaptive_presentation(PopoverPresentation::Adaptive, window, cx),
            PopoverPresentation::BottomSheet
        );
        assert!(read_presentation(None, PresentationDefault::Touch, window, cx).touch_modes);
    });
    // An iPad docking a trackpad: the pointer turns fine, and adaptive anchors again.
    cx.update(|_, cx| rustryx_primitives::modality::set_primary_pointer_coarse(Some(false), cx));
    cx.update(|window, cx| {
        assert_eq!(
            adaptive_presentation(PopoverPresentation::Adaptive, window, cx),
            PopoverPresentation::Popover
        );
        assert!(!read_presentation(None, PresentationDefault::Touch, window, cx).touch_modes);
        // A fixed presentation stays as it is.
        assert_eq!(
            adaptive_presentation(PopoverPresentation::BottomSheet, window, cx),
            PopoverPresentation::BottomSheet
        );
    });
}

#[test]
fn holds_what_the_opening_chose_until_it_is_released() {
    let mut hold = PresentationHold::default();
    assert!(!hold.is_held());
    hold.take(PresentationSurface::BottomSheet);
    assert_eq!(hold.current(), Some(PresentationSurface::BottomSheet));
    hold.release();
    assert_eq!(hold.current(), None);
}

#[test]
fn holds_a_false_choice_as_held() {
    let mut hold = PresentationHold::default();
    hold.take(false);
    assert!(hold.is_held());
    assert_eq!(hold.current(), Some(false));
}

#[test]
fn holds_the_presentation_from_the_frame_that_opens_it_whatever_it_becomes_until_the_next_opening()
{
    let mut hold = PresentationHold::default();
    // Closed: the surface follows the device.
    assert!(!hold.follow(false, false, || false));
    // The opening frame chooses the sheet; later frames keep it, whatever they read.
    assert!(hold.follow(true, false, || true));
    assert!(hold.follow(true, true, || false));
    // Closing plays out as the sheet.
    assert!(hold.follow(false, true, || false));
    // The next opening chooses again.
    assert!(!hold.follow(true, false, || false));
}
