//! BottomSheetSwitcher, upstream `BottomSheet/BottomSheetSwitcher.tsx` (deprecated
//! upstream, DEP-0015: prefer one BottomSheet per step, opened by one state value):
//! steps through sheets, one active at a time.
//!
//! - Zero or one step is interactive for each `active_sheet` (FR1); the others are
//!   hidden, or retained inert through a handoff.
//! - A change of step is a handoff (FR2, FR3): the entering step enters above the
//!   retained one, a taller retained step aligns down to meet it, and the retained
//!   step fades once both motions end. Under Reduce Motion nothing travels.
//! - Closing retains the outgoing step through its exit; focus returns to what had
//!   it when the flow opened (FR4).
//! - The active step's `purpose` governs Escape, a scrim press and a swipe (FR6);
//!   Escape reaches only the topmost layer (FR7). One backdrop dims the page through
//!   a handoff (FR8). Among other sheets, the flow is one level of the stack (FR10).

use std::rc::Rc;

use gpui::{App, IntoElement, ParentElement, RenderOnce, SharedString, Window, div, prelude::*};

use super::BottomSheet;

type ActiveFn = Rc<dyn Fn(Option<SharedString>, &mut Window, &mut App)>;

/// Steps through sheets, one open at a time, upstream `BottomSheetSwitcher`.
#[derive(IntoElement)]
pub struct BottomSheetSwitcher {
    active_sheet: Option<SharedString>,
    on_active_sheet_change: Option<ActiveFn>,
    on_cancel: Option<crate::overlay::modal::CloseFn>,
    scrim: bool,
    sheets: Vec<BottomSheet>,
}

impl BottomSheetSwitcher {
    /// Upstream `activeSheet`: the open step's `sheet_id`, or `None`.
    pub fn new(active_sheet: Option<SharedString>) -> Self {
        BottomSheetSwitcher {
            active_sheet,
            on_active_sheet_change: None,
            on_cancel: None,
            scrim: true,
            sheets: Vec::new(),
        }
    }

    /// Upstream `onActiveSheetChange`: `None` when the active step dismisses.
    pub fn on_active_sheet_change(
        mut self,
        f: impl Fn(Option<SharedString>, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_active_sheet_change = Some(Rc::new(f));
        self
    }

    /// Upstream `onCancel`: called when Escape reaches the active step, before the step
    /// asks to close per its `purpose`.
    pub fn on_cancel(mut self, f: impl Fn(&mut Window, &mut App) + 'static) -> Self {
        self.on_cancel = Some(Rc::new(f));
        self
    }

    /// Upstream `hasScrim`.
    pub fn scrim(mut self, scrim: bool) -> Self {
        self.scrim = scrim;
        self
    }

    /// Upstream `children`: a step with its `sheet_id`.
    pub fn sheet(mut self, sheet: BottomSheet) -> Self {
        self.sheets.push(sheet);
        self
    }
}

impl RenderOnce for BottomSheetSwitcher {
    fn render(self, _: &mut Window, _: &mut App) -> impl IntoElement {
        let active = self.active_sheet;
        let report = self.on_active_sheet_change;
        // The flow's one name in the sheet stack: its steps together.
        let flow: SharedString = self
            .sheets
            .iter()
            .filter_map(|s| s.sheet_key().cloned())
            .collect::<Vec<_>>()
            .join("|")
            .into();
        div()
            .display_contents()
            .children(self.sheets.into_iter().map(|sheet| {
            let open = sheet.sheet_key().is_some() && sheet.sheet_key() == active.as_ref();
            if sheet.sheet_key().is_none() {
                astryx_primitives::dev_warn_once("switcher-sheet-id", || {
                    "BottomSheet: requires a non-empty sheet_id when nested in BottomSheetSwitcher."
                        .into()
                });
            }
            let report = report.clone();
            let sheet = match &self.on_cancel {
                Some(cancel) => sheet.on_cancel(cancel.clone()),
                None => sheet,
            };
            let step = sheet.sheet_key().cloned();
            let current = active.clone();
            sheet
                .opened(open)
                .scrim(self.scrim)
                .flow(flow.clone())
                .on_open_change(move |open, window, cx| {
                    // Only the active step dismisses the flow.
                    if !open
                        && step.is_some()
                        && step == current
                        && let Some(f) = &report
                    {
                        f(None, window, cx);
                    }
                })
        }))
    }
}
