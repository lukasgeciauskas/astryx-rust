//! Upstream `__tests__/haptics.test.ts`: `playHaptic` through the player an
//! app (or a test, as upstream stubs `navigator.vibrate`) installs.

use std::cell::RefCell;
use std::rc::Rc;

use crate::gestures::haptics::*;

#[test]
fn vibrates_each_feedback_kind_with_its_own_pattern() {
    let calls = Rc::new(RefCell::new(Vec::new()));
    let record = calls.clone();
    set_haptic_player(Some(Rc::new(move |kind: HapticFeedbackKind| {
        record.borrow_mut().push(kind.vibration_pattern().to_vec());
        true
    })));
    assert!(can_vibrate());
    for kind in HapticFeedbackKind::ALL {
        assert!(play_haptic(kind));
        assert_eq!(
            calls.borrow().last().cloned(),
            Some(kind.vibration_pattern().to_vec())
        );
    }
    set_haptic_player(None);
}

#[test]
fn plays_nothing_where_the_platform_cannot_vibrate_or_refuses() {
    set_haptic_player(None);
    assert!(!can_vibrate());
    assert!(!play_haptic(HapticFeedbackKind::ImpactLight));
    set_haptic_player(Some(Rc::new(|_| false)));
    assert!(!play_haptic(HapticFeedbackKind::Selection));
    set_haptic_player(None);
}

#[test]
#[should_panic(expected = "blocked")]
fn lets_an_error_from_the_platform_through_a_vibrate_that_throws_is_not_a_refusal() {
    set_haptic_player(Some(Rc::new(|_| panic!("blocked"))));
    play_haptic(HapticFeedbackKind::Selection);
}
