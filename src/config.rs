use std::env;
use std::net::{IpAddr, SocketAddr};
use std::path::PathBuf;
use std::time::Duration;

use anyhow::{Context, Result};
use chrono_tz::Tz;
use secrecy::SecretString;

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
    pub sync_address: String,
    pub username: String,
    pub password: SecretString,
    pub encryption_key: SecretString,
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
        let sync_address = required_env("ATUIN_SYNC_ADDRESS")?;
        let username = required_env("ATUIN_USERNAME")?;
        let password = required_env("ATUIN_PASSWORD")?.into();
        let encryption_key = required_env("ATUIN_ENCRYPTION_KEY")?.into();

        Ok(Self {
            listen: SocketAddr::new(host, port),
            cache_ttl_seconds,
            timezone,
            atuin_sync: AtuinSyncConfig {
                data_dir,
                interval,
                sync_address,
                username,
                password,
                encryption_key,
            },
        })
    }
}

fn required_env(name: &str) -> Result<String> {
    env::var(name).with_context(|| format!("{name} is required"))
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
