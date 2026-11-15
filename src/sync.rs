use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use atuin_client::ai_session::{AiSessionDatabase, AiSessionStore};
use atuin_client::api_client;
use atuin_client::record::sqlite_store::SqliteStore;
use atuin_client::record::sync::{ClientSource, Operation, SyncSession};
use atuin_client::settings::Settings;
use atuin_common::encryption::paseto_v4;
use atuin_domain::api::LoginRequest;
use atuin_domain::record::RecordTag;
use secrecy::ExposeSecret;
use tracing::{error, info};

use crate::agents::AgentStore;
use crate::config::AtuinSyncConfig;
use crate::history::HistoryStore;
use crate::history_projection::HistoryProjection;

#[derive(Clone)]
pub struct AtuinSync {
    inner: Arc<Inner>,
}

struct Inner {
    settings: Arc<Settings>,
    store: SqliteStore,
    history_projection: HistoryProjection,
    agent_projection: AiSessionDatabase,
    history: HistoryStore,
    agents: AgentStore,
    data_dir: PathBuf,
    interval: Duration,
}

impl AtuinSync {
    pub async fn open(config: AtuinSyncConfig) -> Result<(Self, HistoryStore, AgentStore)> {
        tokio::fs::create_dir_all(&config.data_dir)
            .await
            .with_context(|| format!("create {}", config.data_dir.display()))?;
        let settings = Arc::new(prepare_settings(&config).await?);
        let records_path = config.data_dir.join("records.db");
        let store = SqliteStore::new(&records_path, Duration::from_secs(5))
            .await
            .map_err(|error| anyhow::anyhow!("open Abacus Atuin record store: {error:#}"))?;
        let history_path = config.data_dir.join("history.db");
        let history_projection = HistoryProjection::open(&history_path, &records_path).await?;
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
                agent_projection,
                history: history.clone(),
                agents: agents.clone(),
                data_dir: config.data_dir,
                interval: config.interval,
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

        self.inner
            .history_projection
            .reconcile(&self.inner.store, &key)
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

async fn prepare_settings(config: &AtuinSyncConfig) -> Result<Settings> {
    let mut settings =
        Settings::new().map_err(|error| anyhow::anyhow!("load Atuin settings: {error:#}"))?;
    settings.sync_address = config
        .sync_address
        .parse()
        .context("parse ATUIN_SYNC_ADDRESS")?;

    tokio::fs::create_dir_all(&settings.data_dir)
        .await
        .with_context(|| {
            format!(
                "create Atuin data directory {}",
                settings.data_dir.as_path().display()
            )
        })?;
    install_key(&settings.key_path, &config.encryption_key)?;

    if !settings
        .logged_in()
        .await
        .map_err(|error| anyhow::anyhow!("load Atuin login state: {error:#}"))?
    {
        login(&settings, config).await?;
    }

    Ok(settings)
}

fn install_key(path: &std::path::Path, encoded: &secrecy::SecretString) -> Result<()> {
    let supplied =
        paseto_v4::Key::decode(encoded.expose_secret()).context("decode ATUIN_ENCRYPTION_KEY")?;

    if path.exists() {
        let stored =
            paseto_v4::Key::try_load_from_path(path).context("load stored Atuin encryption key")?;
        anyhow::ensure!(
            stored == supplied,
            "ATUIN_ENCRYPTION_KEY does not match stored key"
        );
        return Ok(());
    }

    supplied
        .try_write_path(path)
        .context("store Atuin encryption key")
}

async fn login(settings: &Settings, config: &AtuinSyncConfig) -> Result<()> {
    let response = api_client::login(
        &settings.sync_address,
        LoginRequest {
            username: config.username.clone(),
            password: config.password.clone(),
            totp_code: None,
        },
        &settings.extra_headers,
    )
    .await
    .map_err(|error| anyhow::anyhow!("log in to Atuin sync server: {error:#}"))?;

    settings
        .meta_store()
        .await
        .map_err(|error| anyhow::anyhow!("open Atuin metadata store: {error:#}"))?
        .save_session(&response.session)
        .await
        .map_err(|error| anyhow::anyhow!("save Atuin session: {error:#}"))
}

#[cfg(test)]
mod tests {
    use secrecy::SecretString;

    use super::install_key;
    use atuin_common::encryption::paseto_v4;

    #[test]
    fn installs_and_preserves_the_account_key() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("key");
        let key = paseto_v4::Key::generate();
        let encoded = SecretString::from(key.encode().dangerously_leak_secret().to_owned());

        install_key(&path, &encoded).unwrap();
        install_key(&path, &encoded).unwrap();

        assert_eq!(paseto_v4::Key::try_load_from_path(&path).unwrap(), key);

        let other = paseto_v4::Key::generate();
        let other = SecretString::from(other.encode().dangerously_leak_secret().to_owned());
        assert!(install_key(&path, &other).is_err());
    }
}
