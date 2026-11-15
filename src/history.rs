use std::collections::{BTreeMap, HashSet};
use std::path::Path;
use std::sync::Arc;

use chrono::{SecondsFormat, Timelike};
use serde::Serialize;
use sqlx::SqlitePool;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use tokio::sync::RwLock;

use crate::period::{
    HistoryPeriod, Period, bucket_local_seconds, format_local_bucket, utc_millis_bounds,
};

#[derive(Clone)]
pub struct HistoryStore {
    pool: SqlitePool,
    timestamps: Arc<RwLock<Arc<[i64]>>>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct BucketCount {
    pub date: String,
    pub count: i64,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct TimeOfDayStats {
    pub hourly: [f64; 24],
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Stats {
    pub total: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_command_at: Option<String>,
}

impl HistoryStore {
    #[cfg(test)]
    pub(crate) fn with_timestamps(timestamps: impl Into<Arc<[i64]>>) -> Self {
        Self {
            pool: SqlitePoolOptions::new()
                .connect_lazy("sqlite::memory:")
                .expect("in-memory SQLite URL is valid"),
            timestamps: Arc::new(RwLock::new(timestamps.into())),
        }
    }

    pub async fn open(path: &Path) -> Result<Self, sqlx::Error> {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(
                SqliteConnectOptions::new()
                    .filename(path)
                    .create_if_missing(false)
                    .read_only(true),
            )
            .await?;
        let store = Self {
            pool,
            timestamps: Arc::new(RwLock::new(Arc::new([]))),
        };
        store.refresh().await?;

        Ok(store)
    }

    pub async fn refresh(&self) -> Result<(), sqlx::Error> {
        let timestamps = sqlx::query_scalar(
            "SELECT timestamp / 1000000 AS timestamp FROM history WHERE deleted_at IS NULL ORDER BY timestamp",
        )
        .fetch_all(&self.pool)
        .await?;
        *self.timestamps.write().await = timestamps.into();

        Ok(())
    }

    pub async fn buckets(&self, opts: &HistoryPeriod) -> Vec<BucketCount> {
        let mut buckets = BTreeMap::<i64, i64>::new();
        let timestamps = self.timestamps.read().await.clone();
        for timestamp in filtered(&timestamps, &opts.period) {
            let bucket = bucket_local_seconds(timestamp, opts.period.timezone, opts.rollup_seconds);
            *buckets.entry(bucket).or_default() += 1;
        }

        buckets
            .into_iter()
            .map(|(bucket, count)| BucketCount {
                date: format_local_bucket(bucket, opts.rollup_seconds, opts.period.timezone),
                count,
            })
            .collect()
    }

    pub async fn stats(&self, period: &Period) -> Stats {
        let timestamps = self.timestamps.read().await.clone();
        let values = filtered(&timestamps, period);
        let total = values.clone().count() as i64;
        let last_command_at = values.last().and_then(|timestamp| {
            chrono::DateTime::from_timestamp_millis(timestamp).map(|timestamp| {
                timestamp
                    .with_timezone(&period.timezone)
                    .to_rfc3339_opts(SecondsFormat::AutoSi, false)
            })
        });

        Stats {
            total,
            last_command_at,
        }
    }

    pub async fn time_of_day(&self, period: &Period) -> TimeOfDayStats {
        let mut dates = HashSet::new();
        let mut counts = [0_u64; 24];
        let timestamps = self.timestamps.read().await.clone();
        for timestamp in filtered(&timestamps, period) {
            let Some(timestamp) = chrono::DateTime::from_timestamp_millis(timestamp) else {
                continue;
            };
            let local = timestamp.with_timezone(&period.timezone);
            dates.insert(local.date_naive());
            counts[local.hour() as usize] += 1;
        }

        let mut hourly = [0.0; 24];
        if !dates.is_empty() {
            for (average, count) in hourly.iter_mut().zip(counts) {
                *average = ((count as f64 / dates.len() as f64) * 100.0).round() / 100.0;
            }
        }

        TimeOfDayStats { hourly }
    }
}

fn filtered<'a>(
    timestamps: &'a [i64],
    period: &Period,
) -> impl DoubleEndedIterator<Item = i64> + Clone + 'a {
    let (start, end) = utc_millis_bounds(period);
    timestamps.iter().copied().filter(move |timestamp| {
        start.is_none_or(|start| *timestamp >= start) && end.is_none_or(|end| *timestamp < end)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;
    use sqlx::Executor;

    async fn store() -> HistoryStore {
        let directory = tempfile::tempdir().unwrap().keep();
        let path = directory.join("history.db");
        let pool = SqlitePoolOptions::new()
            .connect_with(
                SqliteConnectOptions::new()
                    .filename(&path)
                    .create_if_missing(true),
            )
            .await
            .unwrap();
        pool.execute(
            "CREATE TABLE history (timestamp INTEGER NOT NULL, deleted_at INTEGER); \
             INSERT INTO history VALUES \
             (1704099600000000000, NULL), \
             (1704103200000000000, NULL), \
             (1704186000000000000, NULL), \
             (1704186000000000000, 1704186000000000000)",
        )
        .await
        .unwrap();
        pool.close().await;

        HistoryStore::open(&path).await.unwrap()
    }

    #[tokio::test]
    async fn computes_the_existing_api_metrics() {
        let store = store().await;
        let period = Period {
            start: Some(NaiveDate::from_ymd_opt(2024, 1, 1).unwrap()),
            end: Some(NaiveDate::from_ymd_opt(2024, 1, 2).unwrap()),
            timezone: chrono_tz::UTC,
        };
        let buckets = store
            .buckets(&HistoryPeriod {
                period: period.clone(),
                rollup_seconds: 86_400,
            })
            .await;

        assert_eq!(
            buckets,
            vec![
                BucketCount {
                    date: "2024-01-01".to_owned(),
                    count: 2,
                },
                BucketCount {
                    date: "2024-01-02".to_owned(),
                    count: 1,
                },
            ]
        );
        assert_eq!(store.stats(&period).await.total, 3);
        let hourly = store.time_of_day(&period).await.hourly;
        assert_eq!(hourly[9], 1.0);
        assert_eq!(hourly[10], 0.5);
    }
}
