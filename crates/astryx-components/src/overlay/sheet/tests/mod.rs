//! BottomSheet's tests, one module per upstream test file, named after the specs'
//! FR and AR items:
//!
//! - `gestures` holds `useSheetGestures.test.ts`'s model suites, fed by hand;
//! - `panel`, `BottomSheet.test.tsx`, `BottomSheetPanel.test.tsx`,
//!   `BottomSheetKeyboard.test.tsx` and `BottomSheetDragExit.test.tsx`, in a window;
//! - `handoff`, `useSheetGestures.test.ts`'s body handoff suites, through raw
//!   touches and the window's touch scroll;
//! - `stacking`, `BottomSheetStacking.test.tsx`, `BottomSheetReplace.test.tsx`,
//!   `BottomSheetCloseFocus.test.tsx` and `BottomSheetSwitcher.test.tsx`;
//! - `resize`, `BottomSheetResizeDuringSettle.test.tsx`.
//! - `snap_offsets.rs` holds `snapOffsets.test.ts`.
//!
//! Left out as DOM- or server-specific: `BottomSheet.ssr.test.tsx` and
//! `BottomSheetSuspense.test.tsx` (no server render or Suspense in GPUI, FR17), the
//! native `<dialog>` and popover-host checks (`showModal`, `show()`, the top layer,
//! `cancel` events, FR11's per-engine focus probes), `BottomSheetEdgeTint.test.tsx`
//! (WebKit's toolbar sampling of fixed elements; the strip is drawn, invisible),
//! `BottomSheetStacking.announce.test.tsx` (AR5: the stack renders no live region,
//! which holds by construction), the browser specs in `__tests__/`, and the
//! layout-split measurements that read `getComputedStyle`, `scrollHeight` and
//! inline styles.

mod gestures;
mod handoff;
mod harness;
mod panel;
mod resize;
mod stacking;
