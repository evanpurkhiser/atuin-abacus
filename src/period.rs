use chrono::{Duration, LocalResult, Months, NaiveDate, TimeZone, Utc};
use chrono_tz::Tz;
use serde::Deserialize;
use thiserror::Error;

#[derive(Clone, Debug)]
pub struct Period {
    pub start: Option<NaiveDate>,
    pub end: Option<NaiveDate>,
    pub timezone: Tz,
}

#[derive(Clone, Debug)]
pub struct HistoryPeriod {
    pub period: Period,
    pub rollup_seconds: i64,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct PeriodQuery {
    pub start: Option<String>,
    pub end: Option<String>,
    pub period: Option<String>,
    pub rollup: Option<String>,
}

#[derive(Debug, Error)]
pub enum PeriodError {
    #[error("Invalid start date format. Expected YYYY-MM-DD, got: {0}")]
    Start(String),
    #[error("Invalid end date format. Expected YYYY-MM-DD, got: {0}")]
    End(String),
    #[error("Start date must be before or equal to end date")]
    Reversed,
    #[error(
        "Invalid period format. Expected format: <number><unit> (e.g., \"1y\", \"6m\", \"30d\"), got: {0}"
    )]
    Period(String),
    #[error(
        "Invalid rollup format. Expected <number><unit> using s, m, h, d, or w (e.g., \"30s\", \"5m\", \"2h\", \"7d\"), got: {0}"
    )]
    Rollup(String),
}

impl PeriodQuery {
    pub fn parse(&self, timezone: Tz) -> Result<Period, PeriodError> {
        if let Some(value) = &self.period {
            return parse_relative_period(value, timezone);
        }

        let start = self
            .start
            .as_deref()
            .map(|value| parse_date(value).map_err(|_| PeriodError::Start(value.to_owned())))
            .transpose()?;
        let end = self
            .end
            .as_deref()
            .map(|value| parse_date(value).map_err(|_| PeriodError::End(value.to_owned())))
            .transpose()?;

        if start.zip(end).is_some_and(|(start, end)| start > end) {
            return Err(PeriodError::Reversed);
        }

        Ok(Period {
            start,
            end,
            timezone,
        })
    }

    pub fn parse_history(&self, timezone: Tz) -> Result<HistoryPeriod, PeriodError> {
        let period = self.parse(timezone)?;
        let rollup = self.rollup.as_deref().unwrap_or("1d");
        let rollup_seconds =
            parse_rollup(rollup).ok_or_else(|| PeriodError::Rollup(rollup.to_owned()))?;

        Ok(HistoryPeriod {
            period,
            rollup_seconds,
        })
    }
}

fn parse_date(value: &str) -> Result<NaiveDate, chrono::ParseError> {
    NaiveDate::parse_from_str(value, "%Y-%m-%d")
}

fn parse_relative_period(value: &str, timezone: Tz) -> Result<Period, PeriodError> {
    let (amount, unit) = value.split_at(value.len().saturating_sub(1));
    let amount: u32 = amount
        .parse()
        .ok()
        .filter(|amount| *amount > 0)
        .ok_or_else(|| PeriodError::Period(value.to_owned()))?;
    let end = Utc::now().with_timezone(&timezone).date_naive();
    let start = match unit.to_ascii_lowercase().as_str() {
        "y" => end.checked_sub_months(Months::new(amount.saturating_mul(12))),
        "m" => end.checked_sub_months(Months::new(amount)),
        "d" => end.checked_sub_signed(Duration::days(i64::from(amount))),
        _ => None,
    }
    .ok_or_else(|| PeriodError::Period(value.to_owned()))?;

    Ok(Period {
        start: Some(start),
        end: Some(end),
        timezone,
    })
}

pub fn parse_rollup(value: &str) -> Option<i64> {
    let (amount, unit) = value.split_at(value.len().checked_sub(1)?);
    let amount: i64 = amount.parse().ok().filter(|amount| *amount > 0)?;
    let multiplier = match unit.to_ascii_lowercase().as_str() {
        "s" => 1,
        "m" => 60,
        "h" => 60 * 60,
        "d" => 24 * 60 * 60,
        "w" => 7 * 24 * 60 * 60,
        _ => return None,
    };

    amount.checked_mul(multiplier)
}

pub fn format_local_bucket(local_seconds: i64, rollup_seconds: i64, timezone: Tz) -> String {
    let naive = chrono::DateTime::from_timestamp(local_seconds, 0)
        .expect("bucket is a valid timestamp")
        .naive_utc();
    if rollup_seconds % (24 * 60 * 60) == 0 {
        return naive.date().to_string();
    }

    timezone
        .from_local_datetime(&naive)
        .earliest()
        .or_else(|| timezone.from_local_datetime(&naive).latest())
        .map(|date| date.to_rfc3339())
        .unwrap_or_else(|| naive.and_utc().to_rfc3339())
}

pub fn bucket_local_seconds(timestamp_millis: i64, timezone: Tz, rollup_seconds: i64) -> i64 {
    let utc = chrono::DateTime::from_timestamp_millis(timestamp_millis)
        .expect("Atuin timestamps are in chrono's range");
    let local = utc
        .with_timezone(&timezone)
        .naive_local()
        .and_utc()
        .timestamp();
    let monday_origin = 4 * 24 * 60 * 60;

    (local - monday_origin).div_euclid(rollup_seconds) * rollup_seconds + monday_origin
}

pub fn utc_millis_bounds(period: &Period) -> (Option<i64>, Option<i64>) {
    let start = period
        .start
        .and_then(|date| local_midnight_millis(date, period.timezone));
    let end = period.end.and_then(|date| {
        date.succ_opt()
            .and_then(|next| local_midnight_millis(next, period.timezone))
    });

    (start, end)
}

fn local_midnight_millis(date: NaiveDate, timezone: Tz) -> Option<i64> {
    let local = date.and_hms_opt(0, 0, 0)?;
    match timezone.from_local_datetime(&local) {
        LocalResult::Single(timestamp) | LocalResult::Ambiguous(timestamp, _) => {
            Some(timestamp.timestamp_millis())
        }
        LocalResult::None => (1..=24).find_map(|hour| {
            timezone
                .from_local_datetime(&local.checked_add_signed(Duration::hours(hour))?)
                .earliest()
                .map(|timestamp| timestamp.timestamp_millis())
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_rollups() {
        assert_eq!(parse_rollup("90s"), Some(90));
        assert_eq!(parse_rollup("2h"), Some(7_200));
        assert_eq!(parse_rollup("1month"), None);
        assert_eq!(parse_rollup("0d"), None);
    }

    #[test]
    fn relative_period_wins_over_dates() {
        let query = PeriodQuery {
            period: Some("30d".to_owned()),
            start: Some("invalid".to_owned()),
            end: Some("invalid".to_owned()),
            ..Default::default()
        };
        assert!(query.parse(chrono_tz::UTC).is_ok());
    }

    #[test]
    fn day_buckets_are_monday_aligned() {
        let timestamp = chrono::DateTime::parse_from_rfc3339("2024-01-01T09:00:00Z")
            .unwrap()
            .timestamp_millis();
        let bucket = bucket_local_seconds(timestamp, chrono_tz::UTC, 7 * 86_400);
        assert_eq!(
            format_local_bucket(bucket, 7 * 86_400, chrono_tz::UTC),
            "2024-01-01"
        );
    }
}
