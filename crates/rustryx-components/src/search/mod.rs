//! Search sources, upstream `Typeahead/types.ts` and
//! `createStaticSource`: what CommandPalette, and later Typeahead and
//! Tokenizer, ask for items.
//!
//! - [`Searchable`]: an item with an id and a label, maybe in a group.
//! - [`SearchSource`]: answers a query, and an empty query with its
//!   bootstrap items, at once or later (a [`gpui::Task`]); it may show
//!   partial lists first through its [`SearchContext`].
//! - [`static_source`]: a source over a fixed list, matching labels and
//!   keywords.

use std::any::Any;
use std::rc::Rc;

use gpui::{App, SharedString, Task};

/// An item a search can return, upstream `SearchableItem`.
pub trait Searchable: Clone + 'static {
    /// Upstream `id`: unique among the results.
    fn id(&self) -> SharedString;
    /// Upstream `label`.
    fn label(&self) -> SharedString;
    /// Upstream `auxiliaryData.group`: a palette sorts its results into
    /// sections by it.
    fn group(&self) -> Option<SharedString> {
        None
    }
}

/// A ready-made searchable item, upstream `SearchableItem` with its
/// `auxiliaryData`.
#[derive(Clone)]
pub struct SearchItem {
    id: SharedString,
    label: SharedString,
    group: Option<SharedString>,
    keywords: Vec<SharedString>,
    data: Option<Rc<dyn Any>>,
}

impl SearchItem {
    pub fn new(id: impl Into<SharedString>, label: impl Into<SharedString>) -> Self {
        SearchItem {
            id: id.into(),
            label: label.into(),
            group: None,
            keywords: Vec::new(),
            data: None,
        }
    }

    /// Upstream `auxiliaryData.group`.
    pub fn group(mut self, group: impl Into<SharedString>) -> Self {
        self.group = Some(group.into());
        self
    }

    /// Words a [`static_source`] matches besides the label.
    pub fn keywords(mut self, keywords: impl IntoIterator<Item = impl Into<SharedString>>) -> Self {
        self.keywords = keywords.into_iter().map(Into::into).collect();
        self
    }

    /// Upstream `auxiliaryData`: anything else the item carries.
    pub fn data(mut self, data: impl Any) -> Self {
        self.data = Some(Rc::new(data));
        self
    }

    /// The data, as `T`.
    pub fn data_as<T: 'static>(&self) -> Option<&T> {
        self.data.as_ref().and_then(|d| d.downcast_ref())
    }

    pub fn keyword_list(&self) -> &[SharedString] {
        &self.keywords
    }
}

impl Searchable for SearchItem {
    fn id(&self) -> SharedString {
        self.id.clone()
    }

    fn label(&self) -> SharedString {
        self.label.clone()
    }

    fn group(&self) -> Option<SharedString> {
        self.group.clone()
    }
}

/// A source's answer: now, or later. A failed answer is an empty one
/// (upstream FR10).
pub enum SearchAnswer<T> {
    Ready(Vec<T>),
    Pending(Task<anyhow::Result<Vec<T>>>),
}

impl<T> From<Vec<T>> for SearchAnswer<T> {
    fn from(items: Vec<T>) -> Self {
        SearchAnswer::Ready(items)
    }
}

/// Per-search context, upstream `SearchContext`: show a list before the
/// answer settles. The first list shown for a query may replace the rows;
/// later ones keep the rows already shown, in order, and add new ones after
/// them. Calls for a query no longer current are ignored.
type UpdateFn<T> = Rc<dyn Fn(Vec<T>, &mut App)>;

pub struct SearchContext<T> {
    update: UpdateFn<T>,
}

impl<T> Clone for SearchContext<T> {
    fn clone(&self) -> Self {
        SearchContext {
            update: self.update.clone(),
        }
    }
}

impl<T> SearchContext<T> {
    pub(crate) fn new(update: impl Fn(Vec<T>, &mut App) + 'static) -> Self {
        SearchContext {
            update: Rc::new(update),
        }
    }

    /// Upstream `context.update(items)`.
    pub fn update(&self, items: Vec<T>, cx: &mut App) {
        (self.update)(items, cx)
    }
}

/// Where a typeahead or palette gets its items, upstream `SearchSource`.
pub trait SearchSource<T: Searchable>: 'static {
    /// Upstream `search(query, context)`: the complete list for `query`.
    fn search(&self, query: &str, context: &SearchContext<T>, cx: &mut App) -> SearchAnswer<T>;
    /// Upstream `bootstrap()`: the items before anything is typed.
    fn bootstrap(&self, cx: &mut App) -> SearchAnswer<T>;
    /// Upstream `cancel()`: a newer search superseded the last one, or the
    /// surface closed.
    fn cancel(&self) {}
}

type KeywordsFn<T> = Rc<dyn Fn(&T) -> Vec<SharedString>>;

/// A source over a fixed list, upstream `createStaticSource`: a query
/// matches labels (and keywords) containing it, case-insensitively; the
/// bootstrap is the whole list.
pub struct StaticSource<T> {
    items: Vec<T>,
    keywords: Option<KeywordsFn<T>>,
}

/// Upstream `createStaticSource(items)`.
pub fn static_source<T: Searchable>(items: impl IntoIterator<Item = T>) -> StaticSource<T> {
    StaticSource {
        items: items.into_iter().collect(),
        keywords: None,
    }
}

impl<T: Searchable> StaticSource<T> {
    /// Upstream `options.keywords`: more words to match per item.
    pub fn keywords(mut self, f: impl Fn(&T) -> Vec<SharedString> + 'static) -> Self {
        self.keywords = Some(Rc::new(f));
        self
    }

    /// The items matching `query`.
    pub fn matches(&self, query: &str) -> Vec<T> {
        let lower = query.trim().to_lowercase();
        if lower.is_empty() {
            return self.items.clone();
        }
        self.items
            .iter()
            .filter(|item| {
                item.label().to_lowercase().contains(&lower)
                    || self
                        .keywords
                        .as_ref()
                        .is_some_and(|f| f(item).iter().any(|k| k.to_lowercase().contains(&lower)))
            })
            .cloned()
            .collect()
    }
}

impl StaticSource<SearchItem> {
    /// Matches each item's own [`SearchItem::keywords`] too.
    pub fn with_item_keywords(self) -> Self {
        self.keywords(|item: &SearchItem| item.keyword_list().to_vec())
    }
}

impl<T: Searchable> SearchSource<T> for StaticSource<T> {
    fn search(&self, query: &str, _: &SearchContext<T>, _: &mut App) -> SearchAnswer<T> {
        SearchAnswer::Ready(self.matches(query))
    }

    fn bootstrap(&self, _: &mut App) -> SearchAnswer<T> {
        SearchAnswer::Ready(self.items.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_static_source_matches_labels_and_keywords() {
        let source = static_source([
            SearchItem::new("new", "New file").keywords(["create"]),
            SearchItem::new("open", "Open file"),
            SearchItem::new("theme", "Toggle theme"),
        ]);
        let ids = |items: Vec<SearchItem>| items.iter().map(|i| i.id()).collect::<Vec<_>>();
        assert_eq!(ids(source.matches("  FILE ")), ["new", "open"]);
        assert_eq!(
            ids(source.matches("")).len(),
            3,
            "an empty query is everything"
        );
        assert!(source.matches("create").is_empty(), "keywords are opt-in");
        let source = source.with_item_keywords();
        assert_eq!(ids(source.matches("create")), ["new"]);
    }
}
