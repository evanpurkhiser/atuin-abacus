use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;

use serde::Serialize;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::{Row, SqlitePool};
use tokio::sync::RwLock;

use crate::period::{
    TimeSeriesPeriod, bucket_local_seconds, format_local_bucket, utc_millis_bounds,
};

const TEXT_BLOCK: i64 = 1;
const SUMMARY_BLOCK: i64 = 1 << 6;

#[derive(Clone)]
pub struct AgentStore {
    pool: SqlitePool,
    events: Arc<RwLock<Arc<[AgentEvent]>>>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct AgentBucket {
    pub date: String,
    pub prompts: u64,
    pub responses: u64,
}

#[derive(Clone, Copy, Debug)]
struct AgentEvent {
    timestamp: i64,
    kind: AgentEventKind,
}

#[derive(Clone, Copy, Debug)]
enum AgentEventKind {
    Prompt,
    Response,
}

impl AgentStore {
    #[cfg(test)]
    pub(crate) fn empty() -> Self {
        Self::with_event_timestamps([], [])
    }

    #[cfg(test)]
    pub(crate) fn with_event_timestamps(
        prompts: impl IntoIterator<Item = i64>,
        responses: impl IntoIterator<Item = i64>,
    ) -> Self {
        let events = prompts
            .into_iter()
            .map(|timestamp| AgentEvent {
                timestamp,
                kind: AgentEventKind::Prompt,
            })
            .chain(responses.into_iter().map(|timestamp| AgentEvent {
                timestamp,
                kind: AgentEventKind::Response,
            }))
            .collect::<Vec<_>>();

        Self {
            pool: SqlitePoolOptions::new()
                .connect_lazy("sqlite::memory:")
                .expect("in-memory SQLite URL is valid"),
            events: Arc::new(RwLock::new(events.into())),
        }
    }

    pub async fn open(path: &Path) -> Result<Self, sqlx::Error> {
        let options = SqliteConnectOptions::new()
            .filename(path)
            .create_if_missing(false)
            .read_only(true);
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(options)
            .await?;
        let store = Self {
            pool,
            events: Arc::new(RwLock::new(Arc::new([]))),
        };
        store.refresh().await?;
        Ok(store)
    }

    pub async fn refresh(&self) -> Result<(), sqlx::Error> {
        let rows = sqlx::query(
            r#"
WITH classified AS (
    SELECT
        CASE
            WHEN role.value = '"User"' THEN 'prompt'
            ELSE 'response'
        END AS kind,
        m.timestamp,
        COALESCE(s.root_harness, s.harness) AS root_harness,
        COALESCE(s.root_session_id, s.session_id) AS root_session,
        CASE
            WHEN role.value = '"User"' THEN m.source_id
            ELSE COALESCE(m.turn_id, m.source_id)
        END AS event_id
    FROM messages m
    JOIN sessions s ON s.id = m.session
    JOIN interned role ON role.id = m.role
    WHERE NOT (
          s.parent_session_id IS NOT NULL
          AND (s.parent_kind IS 0 OR s.parent_kind IS NULL)
      )
      AND (
          (role.value = '"User"' AND (m.blocks & ?) != 0)
          OR
          (role.value = '"Assistant"' AND (m.blocks & ?) != 0)
      )
),
events AS (
    SELECT kind, MIN(timestamp) AS timestamp
    FROM classified
    GROUP BY kind, root_harness, root_session, event_id
)
SELECT kind, timestamp
FROM events
ORDER BY timestamp
"#,
        )
        .bind(TEXT_BLOCK)
        .bind(TEXT_BLOCK | SUMMARY_BLOCK)
        .fetch_all(&self.pool)
        .await?
        .into_iter()
        .map(|row| {
            let kind = match row.try_get::<&str, _>("kind")? {
                "prompt" => AgentEventKind::Prompt,
                "response" => AgentEventKind::Response,
                _ => unreachable!("query returns a known event kind"),
            };
            Ok(AgentEvent {
                timestamp: row.try_get("timestamp")?,
                kind,
            })
        })
        .collect::<Result<Vec<_>, sqlx::Error>>()?;
        *self.events.write().await = rows.into();
        Ok(())
    }

    pub async fn buckets(&self, opts: &TimeSeriesPeriod) -> Vec<AgentBucket> {
        let (start, end) = utc_millis_bounds(&opts.period);
        let mut buckets: BTreeMap<i64, AgentBucket> = BTreeMap::new();
        let events = self.events.read().await.clone();
        for event in events.iter().filter(|event| {
            start.is_none_or(|start| event.timestamp >= start)
                && end.is_none_or(|end| event.timestamp < end)
        }) {
            let bucket =
                bucket_local_seconds(event.timestamp, opts.period.timezone, opts.rollup_seconds);
            let entry = buckets.entry(bucket).or_insert_with(|| AgentBucket {
                date: format_local_bucket(bucket, opts.rollup_seconds, opts.period.timezone),
                ..Default::default()
            });
            match event.kind {
                AgentEventKind::Prompt => entry.prompts += 1,
                AgentEventKind::Response => entry.responses += 1,
            }
        }

        buckets.into_values().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::period::{Period, TimeSeriesPeriod};
    use sqlx::Executor;

    #[tokio::test]
    async fn counts_prompts_and_visible_responses_once_per_root_turn() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("threads.db");
        let options = SqliteConnectOptions::new()
            .filename(&path)
            .create_if_missing(true);
        let pool = SqlitePoolOptions::new()
            .connect_with(options)
            .await
            .unwrap();
        pool.execute(
            r#"
CREATE TABLE interned (id INTEGER PRIMARY KEY, value TEXT NOT NULL UNIQUE);
CREATE TABLE sessions (
    id INTEGER PRIMARY KEY,
    harness INTEGER NOT NULL,
    session_id TEXT NOT NULL,
    parent_session_id TEXT,
    parent_kind INTEGER,
    root_harness INTEGER,
    root_session_id TEXT
);
CREATE TABLE messages (
    id BLOB NOT NULL,
    session INTEGER NOT NULL,
    source_id TEXT NOT NULL,
    timestamp INTEGER NOT NULL,
    role INTEGER NOT NULL,
    blocks INTEGER NOT NULL,
    turn_id TEXT
);
INSERT INTO interned (id, value) VALUES (1, '"User"'), (2, '"Assistant"'), (3, '"Tool"');
INSERT INTO sessions VALUES
    (1, 1, 'root', NULL, NULL, 1, 'root'),
    (2, 1, 'fork', 'root', 1, 1, 'root'),
    (3, 1, 'subagent', 'root', 0, 1, 'root'),
    (4, 1, 'legacy-subagent', 'root', NULL, 1, 'root');
INSERT INTO messages VALUES
    (X'01', 1, 'prompt-1', 1704067200000, 1, 1, NULL),
    (X'02', 1, 'reply-a', 1704067260000, 2, 1, 'turn-1'),
    (X'03', 1, 'reply-b', 1704067270000, 2, 1, 'turn-1'),
    (X'04', 2, 'prompt-1', 1704067200000, 1, 1, NULL),
    (X'05', 2, 'reply-a', 1704067260000, 2, 1, 'turn-1'),
    (X'06', 3, 'sub-prompt', 1704067300000, 1, 1, NULL),
    (X'07', 3, 'sub-reply', 1704067310000, 2, 1, 'turn-sub'),
    (X'08', 1, 'tool-result', 1704067320000, 1, 8, NULL),
    (X'09', 4, 'legacy-sub-prompt', 1704067330000, 1, 1, NULL),
    (X'0A', 4, 'legacy-sub-reply', 1704067340000, 2, 1, 'turn-legacy-sub');
"#,
        )
        .await
        .unwrap();
        pool.close().await;

        let store = AgentStore::open(&path).await.unwrap();
        let buckets = store
            .buckets(&TimeSeriesPeriod {
                period: Period {
                    start: None,
                    end: None,
                    timezone: chrono_tz::UTC,
                },
                rollup_seconds: 86_400,
            })
            .await;

        assert_eq!(
            buckets,
            vec![AgentBucket {
                date: "2024-01-01".to_owned(),
                prompts: 1,
                responses: 1,
            }]
        );
    }
}
