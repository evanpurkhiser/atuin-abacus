use std::env;
use std::net::{IpAddr, SocketAddr};
use std::path::PathBuf;
use std::time::Duration;

use anyhow::{Context, Result};
use chrono_tz::Tz;

#[derive(Clone, Debug)]
pub struct Config {
    pub listen: SocketAddr,
    pub cache_ttl_seconds: u64,
    pub timezone: Tz,
    pub atuin_sync: AtuinSyncConfig,
}

#[derive(Clone, Debug)]
pub struct AtuinSyncConfig {
    pub data_dir: PathBuf,
    pub interval: Duration,
}

impl Config {
    pub fn from_env() -> Result<Self> {
        let port = env_u16("PORT", 8_000)?;
        let host: IpAddr = env::var("HOST")
            .unwrap_or_else(|_| "127.0.0.1".to_owned())
            .parse()
            .context("invalid HOST")?;
        let cache_ttl_seconds = env_u64("CACHE_TTL_SECONDS", 300)?;
        let timezone = env::var("TZ")
            .ok()
            .or_else(|| iana_time_zone::get_timezone().ok())
            .unwrap_or_else(|| "UTC".to_owned())
            .parse()
            .context("invalid system timezone")?;
        let data_dir = env::var_os("ABACUS_DATA_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(".local/atuin-abacus"));
        let interval = Duration::from_secs(env_u64("ATUIN_SYNC_INTERVAL_SECONDS", 300)?);

        Ok(Self {
            listen: SocketAddr::new(host, port),
            cache_ttl_seconds,
            timezone,
            atuin_sync: AtuinSyncConfig { data_dir, interval },
        })
    }
}

fn env_u16(name: &str, default: u16) -> Result<u16> {
    env::var(name)
        .map(|value| value.parse().with_context(|| format!("invalid {name}")))
        .unwrap_or(Ok(default))
}

fn env_u64(name: &str, default: u64) -> Result<u64> {
    env::var(name)
        .map(|value| value.parse().with_context(|| format!("invalid {name}")))
        .unwrap_or(Ok(default))
}
