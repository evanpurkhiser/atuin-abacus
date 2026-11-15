use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use atuin_client::ai_session::{AiSessionDatabase, AiSessionStore};
use atuin_client::database::Sqlite as AtuinHistoryDatabase;
use atuin_client::history::store::HistoryRecord;
use atuin_client::history::{HistoryId, Version};
use atuin_client::record::sqlite_store::SqliteStore;
use atuin_client::record::sync::{ClientSource, Operation, SyncSession};
use atuin_client::settings::Settings;
use atuin_common::encryption::paseto_v4;
use atuin_domain::record::{RecordId, RecordTag};
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::{Executor, QueryBuilder, Sqlite, SqlitePool};
use tokio::sync::Mutex;
use tracing::{error, info};

use crate::agents::AgentStore;
use crate::config::AtuinSyncConfig;
use crate::history::HistoryStore;

#[derive(Clone)]
pub struct AtuinSync {
    inner: Arc<Inner>,
}

struct Inner {
    settings: Arc<Settings>,
    store: SqliteStore,
    history_projection: AtuinHistoryDatabase,
    history_state: SqlitePool,
    agent_projection: AiSessionDatabase,
    history: HistoryStore,
    agents: AgentStore,
    data_dir: PathBuf,
    interval: Duration,
    running: Mutex<()>,
}

impl AtuinSync {
    pub async fn open(config: AtuinSyncConfig) -> Result<(Self, HistoryStore, AgentStore)> {
        tokio::fs::create_dir_all(&config.data_dir)
            .await
            .with_context(|| format!("create {}", config.data_dir.display()))?;
        let settings = Arc::new(
            Settings::new().map_err(|error| anyhow::anyhow!("load Atuin settings: {error:#}"))?,
        );
        let records_path = config.data_dir.join("records.db");
        let store = SqliteStore::new(&records_path, Duration::from_secs(5))
            .await
            .map_err(|error| anyhow::anyhow!("open Abacus Atuin record store: {error:#}"))?;
        let history_path = config.data_dir.join("history.db");
        let history_projection = AtuinHistoryDatabase::new(&history_path, Duration::from_secs(5))
            .await
            .map_err(|error| anyhow::anyhow!("open Abacus Atuin history projection: {error:#}"))?;
        let history_state = open_history_state(&history_path, &records_path).await?;
        let history = HistoryStore::open(&history_path)
            .await
            .context("open history projection for analytics")?;

        let threads_path = config.data_dir.join("threads.db");
        let agent_projection = AiSessionDatabase::open(&threads_path)
            .await
            .context("open Abacus Atuin thread projection")?;
        let agents = AgentStore::open(&threads_path)
            .await
            .context("open thread projection for analytics")?;
        let sync = Self {
            inner: Arc::new(Inner {
                settings,
                store,
                history_projection,
                history_state,
                agent_projection,
                history: history.clone(),
                agents: agents.clone(),
                data_dir: config.data_dir,
                interval: config.interval,
                running: Mutex::new(()),
            }),
        };

        Ok((sync, history, agents))
    }

    pub fn spawn(self) {
        tokio::spawn(async move {
            loop {
                if let Err(error) = self.sync_once().await {
                    sentry::capture_error(error.root_cause());
                    error!(?error, "Atuin sync failed");
                }
                tokio::time::sleep(self.inner.interval).await;
            }
        });
    }

    pub async fn sync_once(&self) -> Result<u64> {
        let _running = self.inner.running.lock().await;
        self.sync_once_inner().await
    }

    async fn sync_once_inner(&self) -> Result<u64> {
        let started = Instant::now();
        let key = paseto_v4::Key::try_load_from_path(&self.inner.settings.key_path)
            .context("load Atuin encryption key")?;
        let session = SyncSession::builder()
            .store(self.inner.store.clone())
            .client_source(ClientSource::FromSettings {
                settings: &self.inner.settings,
                caps: None,
            })
            .build()
            .connect()
            .await
            .context("connect to Atuin sync server")?;
        info!(elapsed = ?started.elapsed(), "connected to Atuin sync server");
        let (diff, remote) = session.diff().await.context("diff Atuin record stores")?;
        info!(
            operations = diff.len(),
            elapsed = ?started.elapsed(),
            "diffed Atuin record stores"
        );
        let keyed = session.keyed(&key);
        if let Some(error) = keyed.key_valid_against(&remote).await {
            return Err(error).context("verify Atuin encryption key");
        }
        let operations = SyncSession::operations(diff)?
            .into_iter()
            .filter(|operation| match operation {
                Operation::Download { series, .. } => {
                    matches!(
                        series.tag,
                        RecordTag::History | RecordTag::AiSession | RecordTag::Packfile
                    )
                }
                Operation::Upload { .. } | Operation::Noop { .. } => false,
            })
            .collect();
        let (_, downloaded) = keyed
            .sync_remote(operations)
            .await
            .context("download Atuin records")?;
        info!(
            downloaded = downloaded.len(),
            elapsed = ?started.elapsed(),
            "downloaded Atuin records"
        );

        reconcile_history(
            &self.inner.store,
            &self.inner.history_projection,
            &self.inner.history_state,
            &key,
        )
        .await?;

        let host_id = self
            .inner
            .settings
            .host_id()
            .await
            .map_err(|error| anyhow::anyhow!("load Atuin host id: {error:#}"))?;
        let ai_store = AiSessionStore::builder()
            .store(self.inner.store.clone())
            .host_id(host_id)
            .key(key)
            .build();
        let projected = ai_store
            .reproject(&self.inner.agent_projection)
            .await
            .context("project Atuin agent sessions")?;
        self.inner
            .history
            .refresh()
            .await
            .context("refresh command activity snapshot")?;
        self.inner
            .agents
            .refresh()
            .await
            .context("refresh agent activity snapshot")?;
        info!(
            downloaded = downloaded.len(),
            ?projected,
            elapsed = ?started.elapsed(),
            data_dir = %self.inner.data_dir.display(),
            "Atuin sync complete"
        );

        Ok(downloaded.len() as u64)
    }
}

async fn open_history_state(path: &Path, records_path: &Path) -> Result<SqlitePool> {
    let attached_path = records_path.to_string_lossy().into_owned();
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .after_connect(move |connection, _| {
            let attached_path = attached_path.clone();
            Box::pin(async move {
                sqlx::query("ATTACH DATABASE ? AS record_store")
                    .bind(attached_path)
                    .execute(connection)
                    .await?;
                Ok(())
            })
        })
        .connect_with(
            SqliteConnectOptions::new()
                .filename(path)
                .create_if_missing(false),
        )
        .await
        .context("open history projection state")?;
    pool.execute(
        "CREATE TABLE IF NOT EXISTS abacus_projected_history_records (id TEXT PRIMARY KEY)",
    )
    .await
    .context("initialize history projection state")?;
    pool.execute("CREATE TABLE IF NOT EXISTS abacus_history_tombstones (id TEXT PRIMARY KEY)")
        .await
        .context("initialize history tombstones")?;

    Ok(pool)
}

async fn reconcile_history(
    records: &SqliteStore,
    history_projection: &AtuinHistoryDatabase,
    state: &SqlitePool,
    key: &paseto_v4::Key,
) -> Result<()> {
    let pending = sqlx::query_scalar::<_, String>(
        "SELECT records.id
         FROM record_store.store AS records
         LEFT JOIN abacus_projected_history_records AS projected ON projected.id = records.id
         WHERE records.tag = 'history' AND projected.id IS NULL",
    )
    .fetch_all(state)
    .await
    .context("find history records pending projection")?
    .into_iter()
    .map(|id| {
        uuid::Uuid::parse_str(&id)
            .map(RecordId::from)
            .map_err(|error| anyhow::anyhow!("invalid history record id {id}: {error}"))
    })
    .collect::<Result<Vec<_>>>()?;

    if pending.is_empty() {
        return Ok(());
    }

    let mut tombstones: HashSet<HistoryId> =
        sqlx::query_scalar::<_, String>("SELECT id FROM abacus_history_tombstones")
            .fetch_all(state)
            .await
            .context("load history tombstones")?
            .into_iter()
            .map(|id| {
                id.parse()
                    .map_err(|error| sqlx::Error::Decode(Box::new(error)))
            })
            .collect::<Result<_, _>>()?;
    let mut skipped = 0;
    let mut projected = 0;
    for pending in pending.chunks(500) {
        let mut creates = Vec::new();
        let mut new_tombstones = Vec::new();
        let mut applied = Vec::new();
        for record in records
            .get_many(pending)
            .await
            .map_err(|error| anyhow::anyhow!("load pending Atuin history records: {error:#}"))?
        {
            let id = record.id;
            let version = record.version.clone();
            let decoded = Version::from_name(version.as_str())
                .context("unsupported Atuin history record version")
                .and_then(|_| record.decrypt(key).context("decrypt Atuin history record"))
                .and_then(|record| {
                    HistoryRecord::deserialize(&record.data, version.as_str())
                        .map_err(|error| anyhow::anyhow!(error))
                        .context("decode Atuin history record")
                });

            match decoded {
                Ok(HistoryRecord::Create(history)) => creates.push(history),
                Ok(HistoryRecord::Delete(id)) => {
                    if tombstones.insert(id) {
                        new_tombstones.push(id);
                    }
                }
                Err(error) => {
                    skipped += 1;
                    tracing::warn!(record_id = %id, ?error, "skipping unreadable Atuin history record");
                    continue;
                }
            }
            applied.push(id);
        }

        creates.retain(|history| !tombstones.contains(&history.id));
        history_projection
            .save_bulk(&creates)
            .await
            .map_err(|error| anyhow::anyhow!("project Atuin history creates: {error:#}"))?;
        history_projection
            .delete_rows(new_tombstones.iter().copied())
            .await
            .map_err(|error| anyhow::anyhow!("project Atuin history tombstones: {error:#}"))?;

        checkpoint_history(state, &applied, &new_tombstones).await?;
        projected += applied.len();
    }
    info!(projected, skipped, "reconciled Atuin history projection");

    Ok(())
}

async fn checkpoint_history(
    state: &SqlitePool,
    applied: &[RecordId],
    tombstones: &[HistoryId],
) -> Result<()> {
    let mut transaction = state
        .begin()
        .await
        .context("begin history projection checkpoint")?;
    if !applied.is_empty() {
        let mut query = QueryBuilder::<Sqlite>::new(
            "INSERT OR IGNORE INTO abacus_projected_history_records (id) ",
        );
        query.push_values(applied, |mut row, id| {
            row.push_bind(id.to_string());
        });
        query
            .build()
            .execute(&mut *transaction)
            .await
            .context("save history projection state")?;
    }
    if !tombstones.is_empty() {
        let mut query =
            QueryBuilder::<Sqlite>::new("INSERT OR IGNORE INTO abacus_history_tombstones (id) ");
        query.push_values(tombstones, |mut row, id| {
            row.push_bind(id.to_string());
        });
        query
            .build()
            .execute(&mut *transaction)
            .await
            .context("save history tombstones")?;
    }
    transaction
        .commit()
        .await
        .context("commit history projection checkpoint")?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use atuin_client::history::History;
    use atuin_client::history::store::HistoryStore as AtuinHistoryStore;
    use atuin_domain::record::{CmdOrigin, HostId};
    use time::OffsetDateTime;

    #[tokio::test]
    async fn attaches_the_record_store_to_replacement_connections() {
        let directory = tempfile::tempdir().unwrap();
        let records_path = directory.path().join("records.db");
        let history_path = directory.path().join("history.db");
        SqliteStore::new(&records_path, Duration::from_secs(5))
            .await
            .unwrap();
        AtuinHistoryDatabase::new(&history_path, Duration::from_secs(5))
            .await
            .unwrap();
        let state = open_history_state(&history_path, &records_path)
            .await
            .unwrap();

        state.acquire().await.unwrap().close().await.unwrap();

        let tables: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM record_store.sqlite_master WHERE type = 'table' AND name = 'store'",
        )
        .fetch_one(&state)
        .await
        .unwrap();
        assert_eq!(tables, 1);
    }

    #[tokio::test]
    async fn history_tombstones_win_over_late_creates() {
        let directory = tempfile::tempdir().unwrap();
        let records_path = directory.path().join("records.db");
        let history_path = directory.path().join("history.db");
        let records = SqliteStore::new(&records_path, Duration::from_secs(5))
            .await
            .unwrap();
        let key: paseto_v4::Key = [0; 32].into();
        let writer =
            AtuinHistoryStore::new(records.clone(), HostId(uuid::Uuid::now_v7()), key.clone());
        let history = History {
            id: "018cd4fe81757cd2aee65cd7861f9c81".parse().unwrap(),
            timestamp: OffsetDateTime::from_unix_timestamp(1_704_067_200).unwrap(),
            duration: 100,
            exit: 0,
            command: "ls".to_owned(),
            cwd: "/tmp".to_owned(),
            session: "018cd4fead897597852527a31c998059".to_owned(),
            cmd_origin: CmdOrigin::try_from("test:user").unwrap(),
            author: "user".to_owned(),
            intent: None,
            deleted_at: None,
            shell: Some("bash".to_owned()),
            author_kind: None,
        };
        writer.push(history.clone()).await.unwrap();
        writer.delete(history.id).await.unwrap();

        let projection = AtuinHistoryDatabase::new(&history_path, Duration::from_secs(5))
            .await
            .unwrap();
        let state = open_history_state(&history_path, &records_path)
            .await
            .unwrap();
        reconcile_history(&records, &projection, &state, &key)
            .await
            .unwrap();
        assert_eq!(history_count(&state).await, 0);

        writer.push(history).await.unwrap();
        reconcile_history(&records, &projection, &state, &key)
            .await
            .unwrap();
        assert_eq!(history_count(&state).await, 0);
    }

    async fn history_count(state: &SqlitePool) -> i64 {
        sqlx::query_scalar("SELECT count(*) FROM history")
            .fetch_one(state)
            .await
            .unwrap()
    }
}
