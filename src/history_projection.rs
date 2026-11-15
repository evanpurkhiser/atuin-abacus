use std::collections::HashSet;
use std::path::Path;
use std::time::Duration;

use anyhow::{Context, Result};
use atuin_client::database::Sqlite as AtuinHistoryDatabase;
use atuin_client::history::store::HistoryRecord;
use atuin_client::history::{HistoryId, Version};
use atuin_client::record::sqlite_store::SqliteStore;
use atuin_common::encryption::paseto_v4;
use atuin_domain::record::RecordId;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::{Executor, QueryBuilder, Sqlite, SqlitePool};
use tracing::info;

pub(crate) struct HistoryProjection {
    database: AtuinHistoryDatabase,
    state: SqlitePool,
}

impl HistoryProjection {
    pub(crate) async fn open(path: &Path, records_path: &Path) -> Result<Self> {
        let database = AtuinHistoryDatabase::new(path, Duration::from_secs(5))
            .await
            .map_err(|error| anyhow::anyhow!("open Abacus Atuin history projection: {error:#}"))?;
        let attached_path = records_path.to_string_lossy().into_owned();
        let state = SqlitePoolOptions::new()
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
        state
            .execute(
                "CREATE TABLE IF NOT EXISTS abacus_projected_history_records \
                 (id TEXT PRIMARY KEY)",
            )
            .await
            .context("initialize history projection state")?;
        state
            .execute("CREATE TABLE IF NOT EXISTS abacus_history_tombstones (id TEXT PRIMARY KEY)")
            .await
            .context("initialize history tombstones")?;

        Ok(Self { database, state })
    }

    pub(crate) async fn reconcile(
        &self,
        records: &SqliteStore,
        key: &paseto_v4::Key,
    ) -> Result<()> {
        let pending = sqlx::query_scalar::<_, String>(
            "SELECT records.id
             FROM record_store.store AS records
             LEFT JOIN abacus_projected_history_records AS projected
                ON projected.id = records.id
             WHERE records.tag = 'history' AND projected.id IS NULL",
        )
        .fetch_all(&self.state)
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
                .fetch_all(&self.state)
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
                        tracing::warn!(
                            record_id = %id,
                            ?error,
                            "skipping unreadable Atuin history record"
                        );
                        continue;
                    }
                }
                applied.push(id);
            }

            creates.retain(|history| !tombstones.contains(&history.id));
            self.database
                .save_bulk(&creates)
                .await
                .map_err(|error| anyhow::anyhow!("project Atuin history creates: {error:#}"))?;
            self.database
                .delete_rows(new_tombstones.iter().copied())
                .await
                .map_err(|error| anyhow::anyhow!("project Atuin history tombstones: {error:#}"))?;

            self.checkpoint(&applied, &new_tombstones).await?;
            projected += applied.len();
        }
        info!(projected, skipped, "reconciled Atuin history projection");

        Ok(())
    }

    async fn checkpoint(&self, applied: &[RecordId], tombstones: &[HistoryId]) -> Result<()> {
        let mut transaction = self
            .state
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
            let mut query = QueryBuilder::<Sqlite>::new(
                "INSERT OR IGNORE INTO abacus_history_tombstones (id) ",
            );
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
        let projection = HistoryProjection::open(&history_path, &records_path)
            .await
            .unwrap();

        projection
            .state
            .acquire()
            .await
            .unwrap()
            .close()
            .await
            .unwrap();

        let tables: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM record_store.sqlite_master \
             WHERE type = 'table' AND name = 'store'",
        )
        .fetch_one(&projection.state)
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

        let projection = HistoryProjection::open(&history_path, &records_path)
            .await
            .unwrap();
        projection.reconcile(&records, &key).await.unwrap();
        assert_eq!(history_count(&projection.state).await, 0);

        writer.push(history).await.unwrap();
        projection.reconcile(&records, &key).await.unwrap();
        assert_eq!(history_count(&projection.state).await, 0);
    }

    async fn history_count(state: &SqlitePool) -> i64 {
        sqlx::query_scalar("SELECT count(*) FROM history")
            .fetch_one(state)
            .await
            .unwrap()
    }
}
