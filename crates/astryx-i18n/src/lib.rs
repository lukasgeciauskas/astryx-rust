//! Astryx's internationalization runtime: upstream's `i18n` subpackage.
//!
//! - [`I18n`] is upstream's `InternationalizationProvider` value: a locale,
//!   its text direction and calendar, and its messages. Set one for the app
//!   with [`set_i18n`], or for a subtree with [`provider`].
//! - Components read it during render with [`current`] (`useTranslator`,
//!   `useLocale`), [`use_direction`] and [`use_locale`].
//! - Messages are ICU MessageFormat ([`message`]). Astryx ships English and
//!   30 translations (the `locales` feature); keys a catalog lacks fall back
//!   to English, then to the key itself.

mod catalog;
pub mod date;
pub mod datetime;
mod first_strong;
mod local;
pub(crate) mod locale;
mod logical;
pub mod message;
pub mod number;

use std::cell::OnceCell;
use std::collections::HashMap;
use std::rc::Rc;

use astryx_primitives::{Provider, provide, use_context};
use gpui::{App, Global, IntoElement, SharedString};

pub use datetime::{InstantFormat, TimeZoneChoice};
pub use first_strong::first_strong_direction;
pub use locale::{Direction, DurationUnit, LocaleInfo, locale_chain, locale_direction};
pub use logical::LogicalStyled;
pub use message::{Arg, Message};

/// The calendar system dates are shown in, upstream `CalendarSystem`.
/// Values stay Gregorian ISO dates; this is only how they are written.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum CalendarSystem {
    #[default]
    Gregory,
    Iso8601,
    Buddhist,
    Roc,
    Japanese,
    Persian,
    Indian,
    Hebrew,
    Chinese,
    Dangi,
    Coptic,
    Ethiopic,
    Ethioaa,
    IslamicCivil,
    IslamicTbla,
    IslamicUmalqura,
}

impl CalendarSystem {
    /// The BCP 47 calendar identifier (`-u-ca-` value).
    pub fn identifier(self) -> &'static str {
        match self {
            CalendarSystem::Gregory => "gregory",
            CalendarSystem::Iso8601 => "iso8601",
            CalendarSystem::Buddhist => "buddhist",
            CalendarSystem::Roc => "roc",
            CalendarSystem::Japanese => "japanese",
            CalendarSystem::Persian => "persian",
            CalendarSystem::Indian => "indian",
            CalendarSystem::Hebrew => "hebrew",
            CalendarSystem::Chinese => "chinese",
            CalendarSystem::Dangi => "dangi",
            CalendarSystem::Coptic => "coptic",
            CalendarSystem::Ethiopic => "ethiopic",
            CalendarSystem::Ethioaa => "ethioaa",
            CalendarSystem::IslamicCivil => "islamic-civil",
            CalendarSystem::IslamicTbla => "islamic-tbla",
            CalendarSystem::IslamicUmalqura => "islamic-umalqura",
        }
    }
}

#[derive(Clone)]
struct Inner {
    locale: SharedString,
    direction: Direction,
    calendar: CalendarSystem,
    /// App-provided catalogs and overrides, by locale tag.
    messages: HashMap<SharedString, HashMap<SharedString, SharedString>>,
    overrides: HashMap<SharedString, HashMap<SharedString, SharedString>>,
    info: Rc<LocaleInfo>,
    /// The locale's numeric collator, made on first use.
    collator: Rc<OnceCell<Option<icu_collator::CollatorBorrowed<'static>>>>,
}

/// A locale with its direction, calendar and messages: upstream's
/// `InternationalizationProvider` props and context value. Cheap to clone.
#[derive(Clone)]
pub struct I18n(Rc<Inner>);

impl I18n {
    /// A BCP 47 locale (`en`, `pt-BR`, `zh-Hant`). Its direction comes from
    /// its script, its calendar is Gregorian, and Astryx's shipped catalog
    /// for a tag in its chain (`fr-FR`, then `fr`) is used.
    pub fn new(locale: impl Into<SharedString>) -> Self {
        let locale = locale.into();
        Self(Rc::new(Inner {
            direction: locale_direction(&locale),
            calendar: CalendarSystem::Gregory,
            messages: HashMap::new(),
            overrides: HashMap::new(),
            info: Rc::new(LocaleInfo::new(&locale)),
            collator: Rc::default(),
            locale,
        }))
    }

    fn inner_mut(&mut self) -> &mut Inner {
        Rc::make_mut(&mut self.0)
    }

    /// Forces the text direction, upstream `dir`.
    pub fn dir(mut self, direction: Direction) -> Self {
        self.inner_mut().direction = direction;
        self
    }

    /// The calendar dates are written in, upstream `calendar`.
    pub fn calendar(mut self, calendar: CalendarSystem) -> Self {
        self.inner_mut().calendar = calendar;
        self
    }

    /// Adds a catalog for a locale tag, upstream `messages`. It takes
    /// precedence over Astryx's shipped catalog for the same tag.
    pub fn messages<K, V>(
        mut self,
        tag: impl Into<SharedString>,
        entries: impl IntoIterator<Item = (K, V)>,
    ) -> Self
    where
        K: Into<SharedString>,
        V: Into<SharedString>,
    {
        let catalog = self.inner_mut().messages.entry(tag.into()).or_default();
        catalog.extend(entries.into_iter().map(|(k, v)| (k.into(), v.into())));
        self
    }

    /// Replaces individual messages for a locale tag, upstream `overrides`.
    /// Overrides win over every catalog.
    pub fn overrides<K, V>(
        mut self,
        tag: impl Into<SharedString>,
        entries: impl IntoIterator<Item = (K, V)>,
    ) -> Self
    where
        K: Into<SharedString>,
        V: Into<SharedString>,
    {
        let catalog = self.inner_mut().overrides.entry(tag.into()).or_default();
        catalog.extend(entries.into_iter().map(|(k, v)| (k.into(), v.into())));
        self
    }

    /// The locale tag, upstream `useLocale`.
    pub fn locale(&self) -> &SharedString {
        &self.0.locale
    }

    /// The text direction, upstream `useDirection`.
    pub fn direction(&self) -> Direction {
        self.0.direction
    }

    /// The calendar system.
    pub fn calendar_system(&self) -> CalendarSystem {
        self.0.calendar
    }

    /// The locale's number and plural formatting.
    pub fn info(&self) -> &LocaleInfo {
        &self.0.info
    }

    /// The raw message for a key, resolved as upstream does: overrides along
    /// the locale chain, then catalogs along it, then English. `None` when
    /// even English lacks the key.
    pub fn message(&self, key: &str) -> Option<SharedString> {
        let chain = locale_chain(&self.0.locale);
        for tag in &chain {
            if let Some(message) = self.0.overrides.get(tag.as_str()).and_then(|c| c.get(key)) {
                return Some(message.clone());
            }
        }
        for tag in &chain {
            if let Some(message) = self.0.messages.get(tag.as_str()).and_then(|c| c.get(key)) {
                return Some(message.clone());
            }
            if let Some(message) = catalog::shipped(tag, key) {
                return Some(message.into());
            }
        }
        catalog::english(key)
            .or_else(|| local::english(key))
            .map(SharedString::from)
    }

    /// A message with no arguments, upstream `t(key)`. A key no catalog has
    /// is returned as is.
    pub fn t(&self, key: &str) -> SharedString {
        self.format(key, &[])
    }

    /// A message formatted with arguments, upstream `t(key, values)`.
    pub fn format(&self, key: &str, args: &[(&str, Arg<'_>)]) -> SharedString {
        let Some(message) = self.message(key) else {
            missing_key(key, &self.0.locale);
            return SharedString::from(key.to_string());
        };
        if args.is_empty() && !message.contains(['{', '\'']) {
            return message;
        }
        self.format_message(&message, args).into()
    }

    /// Formats any ICU message in this locale. Malformed messages are shown as
    /// written.
    pub fn format_message(&self, message: &str, args: &[(&str, Arg<'_>)]) -> String {
        match Message::parse(message) {
            Ok(parsed) => parsed.format(args, &*self.0.info),
            Err(_) => message.to_string(),
        }
    }

    /// An instant in this locale and calendar, upstream `formatInstant`.
    pub fn format_instant(
        &self,
        unix_seconds: i64,
        format: datetime::InstantFormat,
        zone: &datetime::TimeZoneChoice,
        show_zone: bool,
    ) -> String {
        datetime::format_instant(
            &locale::parse_locale(&self.0.locale),
            unix_seconds,
            format,
            zone,
            show_zone,
            self.0.calendar,
        )
    }

    /// The long name of the zone an instant falls in, upstream
    /// `formatZoneName`.
    pub fn format_zone_name(&self, unix_seconds: i64, zone: &datetime::TimeZoneChoice) -> String {
        datetime::format_zone_name(&locale::parse_locale(&self.0.locale), unix_seconds, zone)
    }

    /// A time relative to now, upstream `formatRelativeTime`.
    pub fn format_relative(&self, ago_seconds: i64, narrow: bool) -> String {
        datetime::format_relative(&locale::parse_locale(&self.0.locale), ago_seconds, narrow)
    }

    /// A percentage in the locale's pattern: `percent(45.)` is "45%" in English.
    pub fn percent(&self, value: f64) -> String {
        self.0.info.percent(value)
    }

    /// A number with the locale's digits and separators.
    pub fn number(&self, value: f64) -> String {
        use message::LocaleFormat as _;
        self.0.info.number(value, Default::default())
    }

    /// Two strings in the order the locale sorts them, runs of digits by their
    /// value ("a2" before "a10"): upstream `useCollator({numeric: true})`,
    /// which a table's sort uses.
    pub fn compare(&self, a: &str, b: &str) -> std::cmp::Ordering {
        let collator = self.0.collator.get_or_init(|| {
            let locale = locale::parse_locale(&self.0.locale);
            let mut prefs = icu_collator::CollatorPreferences::from(&locale);
            prefs.numeric_ordering =
                Some(icu_collator::preferences::CollationNumericOrdering::True);
            icu_collator::Collator::try_new(prefs, Default::default()).ok()
        });
        match collator {
            Some(c) => c.compare(a, b),
            None => a.cmp(b),
        }
    }

    /// Items joined as a narrow "and" list in the locale: "a, b, and c".
    pub fn conjunction<'a>(&self, items: impl IntoIterator<Item = &'a str>) -> String {
        self.0.info.conjunction(items)
    }

    /// The locale's number symbols, read from its number format.
    pub fn number_symbols(&self) -> &number::NumberSymbols {
        self.0.info.number_symbols()
    }

    /// Reads a typed number in this locale (`1,234.5`, `1.234,5`, `-12`,
    /// `1.5e3`, other digit scripts); `None` when the text is not a number.
    pub fn parse_number(&self, text: &str) -> Option<f64> {
        number::parse_number(text, self.number_symbols())
    }

    /// A number as a person edits it: plain digits, the locale's decimal
    /// separator, no grouping.
    pub fn editable_number(&self, value: f64) -> String {
        number::editable_number(value, self.number_symbols())
    }
}

fn missing_key(key: &str, locale: &str) {
    if cfg!(debug_assertions) {
        eprintln!("astryx-i18n: missing key {key} (locale: {locale})");
    }
}

thread_local! {
    static ENGLISH: OnceCell<I18n> = const { OnceCell::new() };
}

/// English with no app catalogs, the default when nothing is set.
pub fn english() -> I18n {
    ENGLISH.with(|cell| cell.get_or_init(|| I18n::new("en")).clone())
}

struct GlobalI18n(I18n);

impl Global for GlobalI18n {}

/// Sets the app's locale, as an `InternationalizationProvider` at the root
/// does. Windows redraw with it on their next frame.
pub fn set_i18n(i18n: I18n, cx: &mut App) {
    cx.set_global(GlobalI18n(i18n));
    cx.refresh_windows();
}

/// The nearest [`provider`]'s value, else the app's, else English. Read it
/// during render.
pub fn current(cx: &App) -> I18n {
    use_context::<I18n>(cx)
        .or_else(|| cx.try_global::<GlobalI18n>().map(|g| g.0.clone()))
        .unwrap_or_else(english)
}

/// The text direction in effect, upstream `useDirection`.
pub fn use_direction(cx: &App) -> Direction {
    current(cx).direction()
}

/// The locale in effect, upstream `useLocale`.
pub fn use_locale(cx: &App) -> SharedString {
    current(cx).locale().clone()
}

/// Sets the locale for a subtree, upstream `InternationalizationProvider`.
pub fn provider(i18n: I18n, child: impl IntoElement) -> Provider<I18n> {
    provide(i18n, child)
}

#[cfg(test)]
mod tests;
