//! The sheet flow's draft (FR15b): one open flow edits one copy of the
//! filters; Done replays its edits, in order, onto the field's latest
//! filters, so a filter the parent added meanwhile survives.

use super::model::{PowerSearchChangeType, PowerSearchFilter};

/// One recorded edit.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum DraftEdit {
    Add(PowerSearchFilter),
    Edit {
        before: PowerSearchFilter,
        after: PowerSearchFilter,
    },
    /// The filters one removal (or Clear all) took out.
    Remove(Vec<PowerSearchFilter>),
}

/// One change Done reports: the list after it, what it did, and the index.
pub(crate) type Report = (Vec<PowerSearchFilter>, PowerSearchChangeType, usize);

/// The draft of one open flow.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct SheetDraft {
    pub at_open: Vec<PowerSearchFilter>,
    pub filters: Vec<PowerSearchFilter>,
    pub edits: Vec<DraftEdit>,
}

impl SheetDraft {
    pub fn open(filters: &[PowerSearchFilter]) -> Self {
        SheetDraft {
            at_open: filters.to_vec(),
            filters: filters.to_vec(),
            edits: Vec::new(),
        }
    }

    /// Adds a filter; its index in the draft.
    pub fn add(&mut self, filter: PowerSearchFilter) -> usize {
        self.filters.push(filter.clone());
        self.edits.push(DraftEdit::Add(filter));
        self.filters.len() - 1
    }

    /// Replaces the filter at `index`.
    pub fn edit(&mut self, index: usize, filter: PowerSearchFilter) {
        if let Some(slot) = self.filters.get_mut(index) {
            let before = std::mem::replace(slot, filter.clone());
            self.edits.push(DraftEdit::Edit {
                before,
                after: filter,
            });
        }
    }

    /// Removes the filter at `index`.
    pub fn remove(&mut self, index: usize) {
        if index < self.filters.len() {
            let removed = self.filters.remove(index);
            self.edits.push(DraftEdit::Remove(vec![removed]));
        }
    }

    /// Clear all (FR12, DEC-4): removes every editable filter, keeping the
    /// read-only ones in their order; the index of the first one removed.
    pub fn clear_all(&mut self) -> Option<usize> {
        let first = self.filters.iter().position(|f| !f.read_only)?;
        let removed: Vec<PowerSearchFilter> = self
            .filters
            .iter()
            .filter(|f| !f.read_only)
            .cloned()
            .collect();
        self.filters.retain(|f| f.read_only);
        self.edits.push(DraftEdit::Remove(removed));
        Some(first)
    }

    /// Whether the draft differs from the filters at open (FR15c).
    pub fn changed(&self) -> bool {
        self.filters != self.at_open
    }

    /// Done (FR15b): the reports, replayed onto `latest`, one per edit that
    /// still finds its target; none for an unchanged draft.
    pub fn replay(&self, latest: &[PowerSearchFilter]) -> Vec<Report> {
        if !self.changed() {
            return Vec::new();
        }
        let mut list = latest.to_vec();
        let mut out = Vec::new();
        for edit in &self.edits {
            match edit {
                DraftEdit::Add(filter) => {
                    list.push(filter.clone());
                    out.push((list.clone(), PowerSearchChangeType::Add, list.len() - 1));
                }
                DraftEdit::Edit { before, after } => {
                    if let Some(at) = list.iter().position(|f| f == before) {
                        list[at] = after.clone();
                        out.push((list.clone(), PowerSearchChangeType::Edit, at));
                    }
                }
                DraftEdit::Remove(removed) => {
                    let mut first: Option<usize> = None;
                    for filter in removed {
                        if let Some(at) = list.iter().position(|f| f == filter) {
                            list.remove(at);
                            first = Some(first.map_or(at, |f: usize| f.min(at)));
                        }
                    }
                    if let Some(at) = first {
                        out.push((list.clone(), PowerSearchChangeType::Remove, at));
                    }
                }
            }
        }
        out
    }
}

/// Upstream `nearestEditableIndex` (FR16): after a removal at
/// `removed`, the nearest row that is a button (the following one on a tie).
#[allow(dead_code)] // FR16's focus rules, the flow's next part, use it.
pub(crate) fn nearest_button_row(
    filters: &[PowerSearchFilter],
    removed: usize,
    is_button: impl Fn(&PowerSearchFilter) -> bool,
) -> Option<usize> {
    let mut nearest: Option<usize> = None;
    for (i, f) in filters.iter().enumerate() {
        if !is_button(f) {
            continue;
        }
        nearest = match nearest {
            None => Some(i),
            Some(n) => {
                let (dc, dn) = (i.abs_diff(removed), n.abs_diff(removed));
                if dc < dn || (dc == dn && i >= removed && n < removed) {
                    Some(i)
                } else {
                    Some(n)
                }
            }
        };
    }
    nearest
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::power_search::model::FilterValue;

    fn f(field: &str, value: &str) -> PowerSearchFilter {
        PowerSearchFilter::new(
            field.to_string(),
            "is",
            FilterValue::String(value.to_string().into()),
        )
    }

    #[test]
    fn fr15b_done_replays_the_edits_onto_the_latest_filters() {
        let mut draft = SheetDraft::open(&[f("a", "1"), f("b", "2")]);
        draft.add(f("c", "3"));
        draft.edit(0, f("a", "9"));
        draft.remove(1);
        // The parent added a filter while the flow was open.
        let latest = vec![f("a", "1"), f("b", "2"), f("z", "0")];
        let reports = draft.replay(&latest);
        let kinds: Vec<_> = reports.iter().map(|r| (r.1, r.2)).collect();
        assert_eq!(
            kinds,
            [
                (PowerSearchChangeType::Add, 3),
                (PowerSearchChangeType::Edit, 0),
                (PowerSearchChangeType::Remove, 1)
            ]
        );
        assert_eq!(
            reports.last().unwrap().0,
            vec![f("a", "9"), f("z", "0"), f("c", "3")],
            "the parent's filter survives"
        );
    }

    #[test]
    fn fr15b_an_edit_whose_target_is_gone_is_skipped_and_an_unchanged_draft_reports_nothing() {
        let mut draft = SheetDraft::open(&[f("a", "1")]);
        draft.edit(0, f("a", "2"));
        assert!(draft.replay(&[]).is_empty(), "the parent removed it");
        let mut same = SheetDraft::open(&[f("a", "1")]);
        same.edit(0, f("a", "2"));
        same.edit(0, f("a", "1"));
        assert!(!same.changed());
        assert!(same.replay(&[f("a", "1")]).is_empty());
    }

    #[test]
    fn fr12_clear_all_keeps_read_only_filters() {
        let mut draft = SheetDraft::open(&[f("a", "1").read_only(true), f("b", "2"), f("c", "3")]);
        assert_eq!(draft.clear_all(), Some(1));
        assert_eq!(draft.filters, vec![f("a", "1").read_only(true)]);
        let reports = draft.replay(&draft.at_open.clone());
        assert_eq!(reports.len(), 1);
        assert_eq!(
            (reports[0].1, reports[0].2),
            (PowerSearchChangeType::Remove, 1)
        );
    }

    #[test]
    fn fr16_focus_goes_to_the_nearest_button_row() {
        let rows = vec![f("a", "1"), f("b", "2").read_only(true), f("c", "3")];
        let button = |x: &PowerSearchFilter| !x.read_only;
        assert_eq!(
            nearest_button_row(&rows, 1, button),
            Some(2),
            "the following one on a tie"
        );
        assert_eq!(nearest_button_row(&rows, 0, button), Some(0));
        assert_eq!(
            nearest_button_row(&[f("a", "1").read_only(true)], 0, button),
            None
        );
    }
}
