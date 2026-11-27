//! The Android constants table, upstream `motion/android/`: how the software keyboard
//! moves on Android, in Chrome with Gboard, as the page sees it (`keyboard.ts`). The
//! UIKit table (`motion::uikit`) holds iOS's keyboard.

/// Upstream `keyboard`: Gboard's slides, and when they start against Chrome's report and
/// a blur. Content that rides the keyboard plays these curves from the slide's start, so it
/// moves in step with Gboard. They are Gboard's own motion, with no motion role.
///
/// The values come from the Android 15 emulator with Chromium 158 and Gboard: a screen
/// recording at 540 × 1200 gives Gboard's top in every frame, beside a marker the page
/// paints on each report (upstream's `@source` notes, Oct 25 2026).
pub mod keyboard {
    use crate::motion::BezierPoints;

    /// How long Gboard takes to slide up, ms: from its first moved frame to its resting
    /// top (20 shows: 250 to 282 ms, 18 of them 264 to 268 ms).
    pub const SHOW_MS: f64 = 267.;
    /// The curve of Gboard's slide up: fast out, a long settle (fits the same 20 shows
    /// within 2.4 px rms).
    pub const SHOW_CURVE: BezierPoints = [0.1, 0., 0., 1.];
    /// How long after Gboard starts to slide up Chrome's report of it fires, ms. A show
    /// rides from the report dated back by this (twelve shows: 14 to 35 ms, median 33,
    /// less the 8 ms an event takes on average to reach the screen).
    pub const SHOW_REPORT_LAG_MS: f64 = 25.;
    /// How long Gboard takes to slide down out of the page, ms (21 hides; this length
    /// with `HIDE_CURVE` fits them within 7.7 px rms).
    pub const HIDE_MS: f64 = 167.;
    /// The curve of Gboard's slide down: a slow first frame, then fast.
    pub const HIDE_CURVE: BezierPoints = [0.3, 0.1, 0.2, 0.9];
    /// How long after the focused field blurs (to nothing that brings up a keyboard)
    /// Gboard starts to slide down, ms (five scripted blurs, median 16).
    pub const HIDE_AFTER_BLUR_MS: f64 = 16.;
    /// How long after Gboard starts to slide down Chrome's report of the hide fires, ms,
    /// measured on blurs (twelve: 42 to 52 ms after the blur, median 49, less
    /// `HIDE_AFTER_BLUR_MS`). A hide nothing predicts, such as Back, reuses it.
    pub const HIDE_REPORT_LAG_MS: f64 = 33.;
}
