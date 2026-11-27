//! Instants in a zone (upstream `zonedDateTime.ts`): the lightweight `ZonedDateTime` the
//! views take their ranges from.

use gpui::SharedString;
use rustryx_i18n::date::PlainDate;

use super::date_math::{plain_date_from_instant, plain_date_to_instant};
use super::types::{Instant, ScheduleDate, ScheduleRange};

/// An instant and the IANA zone it is read in (upstream `ZonedDateTime`).
#[derive(Clone, Debug, PartialEq)]
pub struct ZonedDateTime {
    /// Upstream `instant`: epoch milliseconds.
    pub instant: Instant,
    /// Upstream `timezoneID`.
    pub timezone_id: SharedString,
}

/// A range's two ends (upstream `ZonedDateTimeRange`).
pub type ZonedDateTimeRange = (ZonedDateTime, ZonedDateTime);

impl ZonedDateTime {
    /// Upstream `zonedDateTimeFromInstant`.
    pub fn from_instant(instant: Instant, timezone_id: impl Into<SharedString>) -> Self {
        ZonedDateTime {
            instant,
            timezone_id: timezone_id.into(),
        }
    }

    /// The day it falls on in its zone (upstream `toPlainDate`).
    pub fn to_plain_date(&self) -> PlainDate {
        plain_date_from_instant(self.instant, &self.timezone_id)
    }

    /// The midnight `days` days from its day (upstream `addDays`).
    pub fn add_days(&self, days: i64) -> Self {
        let date = self.to_plain_date().add_days(days);
        Self::from_instant(
            plain_date_to_instant(date, &self.timezone_id, 0., 0.),
            self.timezone_id.clone(),
        )
    }

    /// The midnight of its day (upstream `startOfDay`).
    pub fn start_of_day(&self) -> Self {
        self.add_days(0)
    }
}

/// Upstream `createZonedDateTime`.
pub fn create_zoned_date_time(
    date: ScheduleDate,
    timezone_id: impl Into<SharedString>,
) -> ZonedDateTime {
    ZonedDateTime::from_instant(date, timezone_id)
}

/// A rendered range's ends as zoned times (upstream `scheduleRangeToZonedDateTimeRange`).
pub fn schedule_range_to_zoned_date_time_range(
    range: &ScheduleRange,
    timezone_id: &SharedString,
) -> ZonedDateTimeRange {
    (
        ZonedDateTime::from_instant(range.start, timezone_id.clone()),
        ZonedDateTime::from_instant(range.end, timezone_id.clone()),
    )
}
