//! Upstream `utils/dateParser.ts`: the Gregorian parser a date field falls
//! back to on Enter or blur (AST-111 FR9), so `13/5/2026` in `en-US` still
//! commits 13 May 2026.
//!
//! Its last step is the browser's lenient `new Date(text)`; this port reads
//! the forms that parser accepts for dates (an English month or weekday name
//! anywhere, numbers in month-day-year or year-month-day order, an optional
//! time of day), which is narrower than V8's legacy parser.

use super::format::{DatePart, DateStyle, PartKind, format_date_to_parts};
use super::plain::PlainDate;
use super::text::is_letter;
use crate::CalendarSystem;

/// Whether the locale writes the day before the month (`isLocaleDayFirst`).
pub fn is_locale_day_first(locale: &str) -> bool {
    let parts: Vec<DatePart> = format_date_to_parts(
        PlainDate {
            year: 2000,
            month: 1,
            day: 15,
        },
        DateStyle::NUMERIC,
        locale,
        CalendarSystem::Gregory,
    );
    let day = parts.iter().position(|p| p.kind == PartKind::Day);
    let month = parts.iter().position(|p| p.kind == PartKind::Month);
    matches!((day, month), (Some(d), Some(m)) if d < m)
}

fn month_name(name: &str) -> Option<u8> {
    const MONTHS: [(&str, u8); 25] = [
        ("january", 1),
        ("jan", 1),
        ("february", 2),
        ("feb", 2),
        ("march", 3),
        ("mar", 3),
        ("april", 4),
        ("apr", 4),
        ("may", 5),
        ("june", 6),
        ("jun", 6),
        ("july", 7),
        ("jul", 7),
        ("august", 8),
        ("aug", 8),
        ("september", 9),
        ("sep", 9),
        ("sept", 9),
        ("october", 10),
        ("oct", 10),
        ("november", 11),
        ("nov", 11),
        ("december", 12),
        ("dec", 12),
        ("", 0),
    ];
    let lower = name.to_ascii_lowercase();
    MONTHS
        .iter()
        .find(|(n, _)| !n.is_empty() && *n == lower)
        .map(|(_, m)| *m)
}

fn try_date(year: i32, month: u32, day: u32) -> Option<PlainDate> {
    if year < 1 || month > 12 || day > 31 {
        return None;
    }
    PlainDate::new(year, month as u8, day as u8)
}

fn numeric_date(first: u32, second: u32, year: i32, locale: &str) -> Option<PlainDate> {
    let (day, month) = if first > 12 && second <= 12 {
        (first, second)
    } else if second > 12 && first <= 12 {
        (second, first)
    } else if first > 12 && second > 12 {
        return None;
    } else if is_locale_day_first(locale) {
        (first, second)
    } else {
        (second, first)
    };
    try_date(year, month, day)
}

#[derive(Debug, PartialEq)]
enum Piece {
    Num(String),
    Word(String),
    Sep(char),
    Space,
}

fn pieces(text: &str) -> Vec<Piece> {
    let mut out = Vec::new();
    let mut chars = text.chars().peekable();
    while let Some(&ch) = chars.peek() {
        if ch.is_ascii_digit() {
            let mut run = String::new();
            while let Some(&c) = chars.peek().filter(|c| c.is_ascii_digit()) {
                run.push(c);
                chars.next();
            }
            out.push(Piece::Num(run));
        } else if ch.is_ascii_alphabetic() {
            let mut run = String::new();
            while let Some(&c) = chars.peek().filter(|c| c.is_ascii_alphabetic()) {
                run.push(c);
                chars.next();
            }
            out.push(Piece::Word(run));
        } else if ch.is_whitespace() {
            while chars.peek().is_some_and(|c| c.is_whitespace()) {
                chars.next();
            }
            out.push(Piece::Space);
        } else {
            out.push(Piece::Sep(ch));
            chars.next();
        }
    }
    out
}

/// Reads typed text as a Gregorian date (`parseDateInput`): ISO dates,
/// English month names with or without a year, and numeric dates whose
/// ambiguous order follows the locale. `None` when it is not a date.
pub fn parse_date_input(input: &str, locale: &str) -> Option<PlainDate> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return None;
    }
    let current_year = PlainDate::today().year;
    let all_pieces = pieces(trimmed);
    let tokens: Vec<&Piece> = all_pieces.iter().filter(|p| **p != Piece::Space).collect();
    let num = |p: &Piece| match p {
        Piece::Num(n) => n.parse::<u32>().ok(),
        _ => None,
    };
    let num_len = |p: &Piece| match p {
        Piece::Num(n) => n.len(),
        _ => 0,
    };

    // 1. ISO: YYYY-M-D.
    if let [
        Piece::Num(y),
        Piece::Sep('-'),
        Piece::Num(m),
        Piece::Sep('-'),
        Piece::Num(d),
    ] = all_pieces.as_slice()
        && y.len() == 4
        && m.len() <= 2
        && d.len() <= 2
    {
        return try_date(y.parse().ok()?, m.parse().ok()?, d.parse().ok()?);
    }
    // 2 and 3. Month names: "January 25, 2026", "25 Jan 2026", "Jan 25", "25 Jan".
    let plain: Vec<&Piece> = all_pieces
        .iter()
        .filter(|p| **p != Piece::Sep(','))
        .collect();
    let words_and_numbers = |shape: &[u8]| {
        let filtered: Vec<&&Piece> = plain.iter().filter(|p| ***p != Piece::Space).collect();
        filtered.len() == shape.len()
            && filtered.iter().zip(shape).all(|(p, s)| match (p, s) {
                (Piece::Word(_), b'w') => true,
                (Piece::Num(n), b'd') => n.len() <= 2,
                (Piece::Num(n), b'y') => n.len() == 4,
                _ => false,
            })
    };
    let words: Vec<&String> = tokens
        .iter()
        .filter_map(|p| match p {
            Piece::Word(w) => Some(w),
            _ => None,
        })
        .collect();
    let nums: Vec<&Piece> = tokens
        .iter()
        .copied()
        .filter(|p| matches!(p, Piece::Num(_)))
        .collect();
    if words.len() == 1
        && trimmed
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c.is_whitespace() || c == ',')
        && let Some(month) = month_name(words[0])
    {
        if words_and_numbers(b"wdy") || words_and_numbers(b"dwy") {
            return try_date(num(nums[1])? as i32, month as u32, num(nums[0])?);
        }
        if words_and_numbers(b"wd") || words_and_numbers(b"dw") {
            return try_date(current_year, month as u32, num(nums[0])?);
        }
    }
    // 4 and 5. Numeric with one separator kind: D/M/YYYY, D/M.
    let seps: Vec<char> = all_pieces
        .iter()
        .filter_map(|p| match p {
            Piece::Sep(c) => Some(*c),
            _ => None,
        })
        .collect();
    let only_numbers = all_pieces
        .iter()
        .all(|p| matches!(p, Piece::Num(_) | Piece::Sep('-' | '/' | '.')));
    if only_numbers && seps.iter().all(|c| matches!(c, '-' | '/' | '.')) {
        if all_pieces.len() == 5
            && num_len(&all_pieces[0]) <= 2
            && num_len(&all_pieces[2]) <= 2
            && num_len(&all_pieces[4]) == 4
        {
            if seps[0] != seps[1] {
                return None;
            }
            return numeric_date(
                num(&all_pieces[0])?,
                num(&all_pieces[2])?,
                num(&all_pieces[4])? as i32,
                locale,
            );
        }
        if all_pieces.len() == 3 && num_len(&all_pieces[0]) <= 2 && num_len(&all_pieces[2]) <= 2 {
            return numeric_date(
                num(&all_pieces[0])?,
                num(&all_pieces[2])?,
                current_year,
                locale,
            );
        }
    }
    // 6. The browser's lenient parse, but never bare numbers, and never
    // letter-free text in a day-first locale unless a four-digit year leads.
    if trimmed.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    let letter_free = !trimmed.chars().any(is_letter);
    let year_first =
        matches!(all_pieces.as_slice(), [Piece::Num(y), Piece::Sep(_), ..] if y.len() == 4);
    if is_locale_day_first(locale) && letter_free && !year_first {
        return None;
    }
    lenient_date(&all_pieces)
}

/// The dates `new Date(text)` reads: a month name (any position), weekday
/// names, "am"/"pm", a time of day, and numbers in M/D/Y or Y/M/D order.
fn lenient_date(pieces: &[Piece]) -> Option<PlainDate> {
    let mut month: Option<u32> = None;
    let mut numbers: Vec<(u32, usize)> = Vec::new();
    let mut skip_time = false;
    for (k, piece) in pieces.iter().enumerate() {
        match piece {
            Piece::Word(w) => {
                let lower = w.to_ascii_lowercase();
                if let Some(m) = (lower.len() >= 3)
                    .then(|| {
                        [
                            "jan", "feb", "mar", "apr", "may", "jun", "jul", "aug", "sep", "oct",
                            "nov", "dec",
                        ]
                        .iter()
                        .position(|p| lower.starts_with(p))
                    })
                    .flatten()
                {
                    if month.is_some() {
                        return None;
                    }
                    month = Some(m as u32 + 1);
                } else {
                    let is_weekday = lower.len() >= 3
                        && ["sun", "mon", "tue", "wed", "thu", "fri", "sat"]
                            .iter()
                            .any(|p| lower.starts_with(p));
                    let is_time_word =
                        matches!(lower.as_str(), "am" | "pm" | "z" | "utc" | "gmt" | "t");
                    if !is_weekday && !is_time_word {
                        return None;
                    }
                }
            }
            Piece::Num(n) => {
                if skip_time {
                    continue;
                }
                if matches!(pieces.get(k + 1), Some(Piece::Sep(':'))) {
                    skip_time = true;
                    continue;
                }
                numbers.push((n.parse().ok()?, n.len()));
            }
            Piece::Sep(':') => {}
            Piece::Sep(_) | Piece::Space => skip_time = false,
        }
    }
    let year_of = |(value, len): (u32, usize)| -> i32 {
        if len <= 2 {
            if value < 50 {
                2000 + value as i32
            } else {
                1900 + value as i32
            }
        } else {
            value as i32
        }
    };
    match (month, numbers.as_slice()) {
        (Some(m), [(d, dl), y]) if *dl <= 2 && *d <= 31 => try_date(year_of(*y), m, *d),
        (Some(m), [y, (d, dl)]) if y.1 >= 3 && *dl <= 2 => try_date(year_of(*y), m, *d),
        (Some(m), [(d, _)]) if *d <= 31 => try_date(2001, m, *d),
        (None, [y, (m, _), (d, _)]) if y.1 >= 3 => try_date(year_of(*y), *m, *d),
        (None, [(m, _), (d, _), y]) => try_date(year_of(*y), *m, *d),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(y: i32, m: u8, day: u8) -> Option<PlainDate> {
        PlainDate::new(y, m, day)
    }

    #[test]
    fn reads_iso_month_names_and_numbers() {
        assert_eq!(parse_date_input("2026-01-25", "en"), d(2026, 1, 25));
        assert_eq!(parse_date_input("January 25, 2026", "en"), d(2026, 1, 25));
        assert_eq!(parse_date_input("25 Jan 2026", "en"), d(2026, 1, 25));
        assert_eq!(parse_date_input("1/25/2026", "en-US"), d(2026, 1, 25));
        assert_eq!(parse_date_input("13/5/2026", "en-US"), d(2026, 5, 13));
        assert_eq!(parse_date_input("05/06/2026", "en-GB"), d(2026, 6, 5));
        assert_eq!(parse_date_input("1/25.2026", "en-US"), None);
    }

    #[test]
    fn never_reads_bare_numbers_or_ambiguous_day_first_text() {
        assert_eq!(parse_date_input("2026", "en"), None);
        assert_eq!(parse_date_input("0", "en"), None);
        assert_eq!(parse_date_input("12/01/2", "en-GB"), None);
        assert_eq!(parse_date_input("2026/01/12", "en-GB"), d(2026, 1, 12));
    }

    #[test]
    fn the_lenient_step_reads_what_the_browser_reads() {
        assert_eq!(parse_date_input("Thu Mar 5 2026", "en"), d(2026, 3, 5));
        assert_eq!(parse_date_input("March 5 2026 10:30", "en"), d(2026, 3, 5));
        assert_eq!(parse_date_input("not a date", "en"), None);
    }
}
