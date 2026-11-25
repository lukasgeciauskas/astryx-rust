//! The search engine behind BaseTypeahead (and Tokenizer), upstream
//! `performSearch`, `performBootstrap` and the open and dismiss rules of
//! `BaseTypeahead.tsx`.
//!
//! - A query at or past `min_query_length` graphemes searches after the
//!   debounce; a shorter one closes the list and invalidates work (FR2).
//! - Every search, bootstrap, selection, clear or source change bumps a
//!   generation; answers for an older one are dropped (FR3).
//! - A partial list shows at once; later lists for the query keep the shown
//!   rows first, and the highlight follows its row (FR10).
//! - Dismissed while work is pending, the field stops being busy and the
//!   answer is kept for the next open (FR3, FR6).

use std::rc::Rc;
use std::time::Duration;

use astryx_i18n::{Arg, current as i18n};
use astryx_primitives::{Politeness, announce};
use gpui::{App, Entity, SharedString, Task, Window};
use unicode_segmentation::UnicodeSegmentation;

use crate::search::{SearchAnswer, SearchContext, SearchSource, Searchable};

pub(crate) type OpenFn = Rc<dyn Fn(bool, &mut Window, &mut App)>;
pub(crate) type QueryFn = Rc<dyn Fn(SharedString, &mut Window, &mut App)>;
pub(crate) type EntriesFn<T> = Rc<dyn Fn(&str, &[T]) -> Vec<T>>;

/// A typeahead's search state between frames.
pub(crate) struct Engine<T> {
    pub open: bool,
    pub query: String,
    pub results: Vec<T>,
    /// Rows a caller appends after the source's (a Tokenizer's create row).
    pub entry_count: usize,
    /// The query the shown rows answer.
    pub list_query: Option<String>,
    pub has_searched: bool,
    pub searched_query: String,
    pub loading: bool,
    pub generation: u64,
    pub debounce: Option<Task<()>>,
    pub work: Option<Task<()>>,
    pub highlight: Option<usize>,
    pub keyboard: bool,
    /// The list was dismissed while work was pending.
    pub dismissed: bool,
    /// An answer kept through a dismissal, with the count to announce.
    pub kept: Option<usize>,
    pub kept_answer: bool,
    /// The query whose empty state was announced.
    pub announced_empty: Option<String>,
    pub source: Option<u64>,
    pub on_open_change: Option<OpenFn>,
    /// The menu is a dialog.
    pub dialog_menu: bool,
}

impl<T> Default for Engine<T> {
    fn default() -> Self {
        Engine {
            open: false,
            query: String::new(),
            results: Vec::new(),
            entry_count: 0,
            list_query: None,
            has_searched: false,
            searched_query: String::new(),
            loading: false,
            generation: 0,
            debounce: None,
            work: None,
            highlight: None,
            keyboard: false,
            dismissed: false,
            kept: None,
            kept_answer: false,
            announced_empty: None,
            source: None,
            on_open_change: None,
            dialog_menu: false,
        }
    }
}

/// Whether `query` is shorter than the threshold, in graphemes.
pub(crate) fn below_min(query: &str, min: usize) -> bool {
    query.graphemes(true).count() < min
}

/// What one frame's engine reads: the source and the settings.
pub(crate) struct Search<T: Searchable> {
    pub engine: Entity<Engine<T>>,
    pub source: Rc<dyn SearchSource<T>>,
    pub max_items: usize,
    pub min_query_length: usize,
    pub debounce: Duration,
    pub entries_on_focus: bool,
    /// Ids already chosen (a Tokenizer's tokens): never offered.
    pub exclude: Vec<SharedString>,
    pub query_entries: Option<EntriesFn<T>>,
    /// The label of appended rows (a create row's "Create …").
    pub entry_label: Option<SharedString>,
}

impl<T: Searchable> Search<T> {
    /// Shows the list; a dialog menu shows only through [`Self::show_dialog`]
    /// (FR14).
    pub fn show(&self, window: &mut Window, cx: &mut App) {
        if self.engine.read(cx).dialog_menu {
            return;
        }
        self.show_dialog(window, cx);
    }

    /// Shows the popup, whatever it holds: the controller's open.
    pub fn show_dialog(&self, window: &mut Window, cx: &mut App) {
        let report = self.engine.update(cx, |e, _| {
            if e.open {
                return None;
            }
            e.open = true;
            Some(e.on_open_change.clone())
        });
        if let Some(Some(f)) = report {
            f(true, window, cx);
        }
        window.refresh();
    }

    /// Upstream `popover.hide`: pending work stays, its answer kept.
    pub fn hide(&self, window: &mut Window, cx: &mut App) {
        let report = self.engine.update(cx, |e, _| {
            if !e.open {
                return None;
            }
            e.open = false;
            e.highlight = None;
            if e.loading || e.debounce.is_some() {
                e.dismissed = true;
                e.loading = false;
            }
            Some(e.on_open_change.clone())
        });
        if let Some(Some(f)) = report {
            f(false, window, cx);
        }
        window.refresh();
    }

    /// Upstream `dismiss` on a closed field: drops the scheduled search.
    pub fn dismiss_pending(&self, cx: &mut App) {
        self.engine.update(cx, |e, _| {
            e.debounce = None;
            e.dismissed = true;
            e.loading = false;
        });
    }

    /// Clears the results and invalidates work (a pick, a clear).
    pub fn reset(&self, window: &mut Window, cx: &mut App) {
        self.source.cancel();
        self.engine.update(cx, |e, _| {
            e.generation += 1;
            e.query.clear();
            e.results.clear();
            e.entry_count = 0;
            e.list_query = None;
            e.has_searched = false;
            e.loading = false;
            e.debounce = None;
            e.work = None;
            e.highlight = None;
            e.dismissed = false;
            e.kept = None;
            e.kept_answer = false;
        });
        self.hide(window, cx);
    }

    fn eligible(&self, items: Vec<T>) -> Vec<T> {
        if self.exclude.is_empty() {
            return items;
        }
        items
            .into_iter()
            .filter(|i| !self.exclude.contains(&i.id()))
            .collect()
    }

    /// Shows a list for `query` from generation `generation`; `first` lists
    /// replace the rows, later ones keep the highlight on its row.
    fn show_list(
        &self,
        generation: u64,
        query: &str,
        items: Vec<T>,
        final_list: bool,
        first: bool,
        cx: &mut App,
    ) -> usize {
        let mut fetched = self.eligible(items);
        let hidden = fetched.len().saturating_sub(self.max_items);
        fetched.truncate(self.max_items);
        let entries = match (&self.query_entries, final_list) {
            (Some(f), true) => f(query, &fetched),
            _ => Vec::new(),
        };
        let shown_count = fetched.len() + entries.len();
        self.engine.update(cx, |e, _| {
            if e.generation != generation {
                return;
            }
            let previous_id = (!first)
                .then(|| e.highlight.and_then(|h| e.results.get(h)).map(|i| i.id()))
                .flatten();
            e.results = fetched.into_iter().chain(entries.iter().cloned()).collect();
            e.entry_count = entries.len();
            e.list_query = Some(query.to_string());
            let first_row = (!e.results.is_empty()).then_some(0);
            e.highlight = match previous_id {
                Some(id) => e.results.iter().position(|i| i.id() == id).or(first_row),
                None if first => first_row,
                None => e.highlight.filter(|h| *h < e.results.len()).or(first_row),
            };
        });
        shown_count + hidden
    }

    /// Upstream `performSearch`.
    pub fn search(&self, query: String, window: &mut Window, cx: &mut App) {
        self.source.cancel();
        let generation = self.engine.update(cx, |e, _| {
            e.generation += 1;
            e.searched_query = query.clone();
            e.dismissed = false;
            e.kept = None;
            e.kept_answer = false;
            e.loading = true;
            e.has_searched = true;
            e.debounce = None;
            e.generation
        });
        let shown_any = Rc::new(std::cell::Cell::new(false));
        let context = {
            let this = self.shared();
            let (query, shown_any) = (query.clone(), shown_any.clone());
            SearchContext::new(move |items: Vec<T>, cx: &mut App| {
                let (current, dismissed, typed) = {
                    let e = this.engine.read(cx);
                    (
                        e.generation == generation && e.loading,
                        e.dismissed,
                        e.query.clone(),
                    )
                };
                if !current || items.is_empty() || (dismissed && query != typed) {
                    return;
                }
                let first = !shown_any.get();
                this.show_list(generation, &query, items, false, first, cx);
                shown_any.set(true);
                if !dismissed {
                    this.engine.update(cx, |e, _| e.open = true);
                }
            })
        };
        match self.source.search(&query, &context, cx) {
            SearchAnswer::Ready(items) => {
                self.settle(generation, query, Some(items), shown_any.get(), window, cx)
            }
            SearchAnswer::Pending(task) => {
                let this = self.shared();
                let work = window.spawn(cx, async move |cx| {
                    let answer = task.await.ok();
                    cx.update(|window, cx| {
                        let first_shown = shown_any.get();
                        this.settle(generation, query, answer, first_shown, window, cx)
                    })
                    .ok();
                });
                self.engine.update(cx, |e, _| e.work = Some(work));
            }
        }
        window.refresh();
    }

    fn settle(
        &self,
        generation: u64,
        query: String,
        answer: Option<Vec<T>>,
        partial_shown: bool,
        window: &mut Window,
        cx: &mut App,
    ) {
        let (current, dismissed, typed) = {
            let e = self.engine.read(cx);
            (e.generation == generation, e.dismissed, e.query.clone())
        };
        if !current {
            return;
        }
        self.engine.update(cx, |e, _| {
            e.loading = false;
            e.work = None;
        });
        if dismissed && query != typed {
            return;
        }
        let count = match answer {
            Some(items) => self.show_list(generation, &query, items, true, !partial_shown, cx),
            // FR10: a failed answer keeps the last list shown.
            None if partial_shown => self.engine.read(cx).results.len(),
            None => {
                self.engine.update(cx, |e, _| {
                    e.results.clear();
                    e.highlight = None;
                });
                0
            }
        };
        if dismissed {
            self.engine.update(cx, |e, _| {
                e.kept_answer = true;
                if !query.is_empty() {
                    e.kept = Some(count);
                }
            });
            return;
        }
        if count > 0 || !query.is_empty() {
            self.show(window, cx);
        }
        if !query.is_empty() && count > 0 {
            let text = i18n(cx).format(
                "@astryx.typeahead.resultCount",
                &[("count", Arg::Num(count as f64))],
            );
            announce(text, Politeness::Polite, window, cx);
        }
        window.refresh();
    }

    /// Upstream `performBootstrap`: the entries before anything is typed.
    pub fn bootstrap(&self, window: &mut Window, cx: &mut App) {
        let generation = self.engine.update(cx, |e, _| {
            e.generation += 1;
            e.loading = true;
            e.dismissed = false;
            e.generation
        });
        match self.source.bootstrap(cx) {
            SearchAnswer::Ready(items) => {
                self.settle_bootstrap(generation, Some(items), window, cx)
            }
            SearchAnswer::Pending(task) => {
                let this = self.shared();
                let work = window.spawn(cx, async move |cx| {
                    let answer = task.await.ok();
                    cx.update(|window, cx| this.settle_bootstrap(generation, answer, window, cx))
                        .ok();
                });
                self.engine.update(cx, |e, _| e.work = Some(work));
            }
        }
        window.refresh();
    }

    fn settle_bootstrap(
        &self,
        generation: u64,
        answer: Option<Vec<T>>,
        window: &mut Window,
        cx: &mut App,
    ) {
        if self.engine.read(cx).generation != generation {
            return;
        }
        self.engine.update(cx, |e, _| {
            e.loading = false;
            e.work = None;
        });
        let items = answer.unwrap_or_default();
        let count = self.show_list(generation, "", items, true, true, cx);
        // FR2: a bootstrap with nothing eligible closes the list.
        if count == 0 {
            self.hide(window, cx);
        } else if self.engine.read(cx).query.is_empty() {
            self.show(window, cx);
        }
        window.refresh();
    }

    /// Upstream `handleQueryChange`: the typed text changed.
    pub fn query_changed(&self, query: String, debounce: bool, window: &mut Window, cx: &mut App) {
        self.engine.update(cx, |e, _| {
            e.query = query.clone();
            e.debounce = None;
            e.announced_empty = None;
        });
        if query.is_empty() {
            self.source.cancel();
            self.engine.update(cx, |e, _| {
                e.generation += 1;
                e.loading = false;
                e.results.clear();
                e.list_query = None;
                e.has_searched = false;
                e.highlight = None;
            });
            if self.entries_on_focus {
                self.bootstrap(window, cx);
            } else {
                self.hide(window, cx);
            }
            return;
        }
        if below_min(&query, self.min_query_length) {
            self.source.cancel();
            self.engine.update(cx, |e, _| {
                e.generation += 1;
                e.loading = false;
                e.results.clear();
                e.list_query = None;
                e.has_searched = false;
                e.highlight = None;
            });
            self.hide(window, cx);
            return;
        }
        if !debounce || self.debounce.is_zero() {
            self.search(query, window, cx);
            return;
        }
        let timer = cx.background_executor().timer(self.debounce);
        let this = self.shared();
        let task = window.spawn(cx, async move |cx| {
            timer.await;
            cx.update(|window, cx| {
                if this.engine.read(cx).query == query {
                    this.search(query, window, cx);
                }
            })
            .ok();
        });
        self.engine.update(cx, |e, _| e.debounce = Some(task));
    }

    /// Upstream `openWithKeptAnswer` and `openIfEligible`: an open by
    /// ArrowDown, focus, a press or the handle shows the kept list, or loads
    /// it.
    pub fn open(&self, window: &mut Window, cx: &mut App) {
        let (query, kept, kept_answer, results, list_query, source_changed) = {
            let e = self.engine.read(cx);
            (
                e.query.clone(),
                e.kept,
                e.kept_answer,
                e.results.len(),
                e.list_query.clone(),
                false,
            )
        };
        let _ = source_changed;
        if kept_answer && list_query.as_deref() == Some(query.as_str()) {
            self.engine.update(cx, |e, _| {
                e.kept_answer = false;
                e.kept = None;
                e.dismissed = false;
                if e.highlight.is_none() && !e.results.is_empty() {
                    e.highlight = Some(0);
                }
            });
            if results > 0 || !query.is_empty() {
                self.show(window, cx);
            }
            if let Some(count) = kept.filter(|c| *c > 0) {
                let text = i18n(cx).format(
                    "@astryx.typeahead.resultCount",
                    &[("count", Arg::Num(count as f64))],
                );
                announce(text, Politeness::Polite, window, cx);
            }
            return;
        }
        if !query.is_empty() {
            if below_min(&query, self.min_query_length) {
                return;
            }
            if list_query.as_deref() == Some(query.as_str()) && !self.engine.read(cx).dismissed {
                self.engine.update(cx, |e, _| {
                    if e.highlight.is_none() && !e.results.is_empty() {
                        e.highlight = Some(0);
                    }
                });
                self.show(window, cx);
            } else {
                self.search(query, window, cx);
            }
        } else if self.entries_on_focus {
            self.bootstrap(window, cx);
        }
    }

    fn shared(&self) -> Rc<Search<T>> {
        Rc::new(Search {
            engine: self.engine.clone(),
            source: self.source.clone(),
            max_items: self.max_items,
            min_query_length: self.min_query_length,
            debounce: self.debounce,
            entries_on_focus: self.entries_on_focus,
            exclude: self.exclude.clone(),
            query_entries: self.query_entries.clone(),
            entry_label: self.entry_label.clone(),
        })
    }
}

/// The engine of the typeahead with element id `id`, in the render scope
/// that draws it (a wrapper reads the same one).
pub(crate) fn engine_of<T: Searchable>(
    id: &gpui::ElementId,
    window: &mut Window,
    cx: &mut App,
) -> Entity<Engine<T>> {
    window.use_keyed_state(
        gpui::ElementId::Name(format!("{id}-typeahead").into()),
        cx,
        |_, _| Engine::default(),
    )
}
