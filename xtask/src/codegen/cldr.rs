//! `crates/rustryx-i18n/src/generated/cldr.rs` from `spec/cldr.json`: the CLDR
//! tables the date components read that ICU4X does not carry (upstream's
//! weekday header names and stand-alone month capitals, and the calendar and
//! week-field display names upstream reads from `Intl.DisplayNames`).

use std::path::Path;

use super::{HEADER, read_json};
use anyhow::{Context, Result};

pub fn render(spec: &Path) -> Result<String> {
    let data = read_json(&spec.join("cldr.json"))?;
    let mut out = format!(
        "{HEADER}// Source: {}.\n\n/// Languages whose stand-alone month names start with a capital.\npub const STANDALONE_MONTH_CAPITALS: &[&str] = &[\n",
        data["intl"].as_str().unwrap_or("Intl")
    );
    for language in data["standaloneMonthCapitalLanguages"]
        .as_array()
        .context("standaloneMonthCapitalLanguages")?
    {
        out.push_str(&format!(
            "    {:?},\n",
            language.as_str().context("a language")?
        ));
    }
    out.push_str("];\n\n/// Weekday column headers by locale tag, Sunday first, sorted by tag.\npub const WEEKDAY_HEADER_NAMES: &[(&str, [&str; 7])] = &[\n");
    let weekdays = data["weekdayHeaderNames"]
        .as_object()
        .context("weekdayHeaderNames")?;
    let mut tags: Vec<&String> = weekdays.keys().collect();
    tags.sort();
    for tag in tags {
        let names: Vec<String> = weekdays[tag]
            .as_array()
            .context("seven names")?
            .iter()
            .map(|n| format!("{:?}", n.as_str().unwrap_or_default()))
            .collect();
        out.push_str(&format!("    ({tag:?}, [{}]),\n", names.join(", ")));
    }
    out.push_str("];\n\n/// Calendar display names (`Intl.DisplayNames` type `calendar`) by locale tag,\n/// sorted by tag; each list is sorted by calendar identifier.\npub const CALENDAR_NAMES: &[(&str, &[(&str, &str)])] = &[\n");
    let names = data["calendarNames"].as_object().context("calendarNames")?;
    let mut tags: Vec<&String> = names.keys().collect();
    tags.sort();
    for tag in tags {
        out.push_str(&format!("    ({tag:?}, &[\n"));
        let calendars = names[tag].as_object().context("calendars")?;
        let mut ids: Vec<&String> = calendars.keys().collect();
        ids.sort();
        for id in ids {
            out.push_str(&format!(
                "        ({id:?}, {:?}),\n",
                calendars[id].as_str().unwrap_or_default()
            ));
        }
        out.push_str("    ]),\n");
    }
    out.push_str("];\n\n/// The name of the week-of-year field (`Intl.DisplayNames` type\n/// `dateTimeField`) by locale tag, sorted by tag.\npub const WEEK_OF_YEAR: &[(&str, &str)] = &[\n");
    let weeks = data["weekOfYear"].as_object().context("weekOfYear")?;
    let mut tags: Vec<&String> = weeks.keys().collect();
    tags.sort();
    for tag in tags {
        out.push_str(&format!(
            "    ({tag:?}, {:?}),\n",
            weeks[tag].as_str().unwrap_or("week")
        ));
    }
    out.push_str("];\n");
    Ok(out)
}
