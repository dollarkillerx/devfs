use std::path::PathBuf;

use clap::Parser;
use serde::Deserialize;

use crate::storage::FsStorage;
use crate::types::{AppState, AuthConfig};

#[derive(Parser)]
#[command(name = "devfs", about = "S3-compatible object storage for development")]
struct Cli {
    /// Data directory for storage
    #[arg(long)]
    data_dir: Option<PathBuf>,

    /// Port to listen on
    #[arg(long)]
    port: Option<u16>,

    /// Host to bind to
    #[arg(long)]
    host: Option<String>,

    /// AWS access key (enables auth when set with secret-key)
    #[arg(long)]
    access_key: Option<String>,

    /// AWS secret key (enables auth when set with access-key)
    #[arg(long)]
    secret_key: Option<String>,

    /// Path to config file (default: devfs.toml in current directory)
    #[arg(long)]
    config: Option<PathBuf>,
}

#[derive(Debug, Deserialize, Default)]
struct FileConfig {
    host: Option<String>,
    port: Option<u16>,
    data_dir: Option<PathBuf>,
    auth: Option<FileAuthConfig>,
}

#[derive(Debug, Deserialize)]
struct FileAuthConfig {
    access_key: Option<String>,
    secret_key: Option<String>,
}

pub struct Config {
    pub host: String,
    pub port: u16,
    pub data_dir: PathBuf,
    pub access_key: Option<String>,
    pub secret_key: Option<String>,
}

impl Config {
    pub fn load() -> Self {
        let cli = Cli::parse();

        // Load config file
        let file_config = load_file_config(cli.config.as_deref());

        // Merge: CLI > env > file > default
        let host = cli
            .host
            .or_else(|| std::env::var("DEVFS_HOST").ok())
            .or(file_config.host)
            .unwrap_or_else(|| "127.0.0.1".to_string());

        let port = cli
            .port
            .or_else(|| std::env::var("DEVFS_PORT").ok().and_then(|v| v.parse().ok()))
            .or(file_config.port)
            .unwrap_or(9000);

        let data_dir = cli
            .data_dir
            .or_else(|| std::env::var("DEVFS_DATA_DIR").ok().map(PathBuf::from))
            .or(file_config.data_dir)
            .unwrap_or_else(|| PathBuf::from("./data"));

        let (file_ak, file_sk) = file_config
            .auth
            .map(|a| (a.access_key, a.secret_key))
            .unwrap_or((None, None));

        let access_key = cli
            .access_key
            .or_else(|| std::env::var("DEVFS_ACCESS_KEY").ok())
            .or(file_ak);

        let secret_key = cli
            .secret_key
            .or_else(|| std::env::var("DEVFS_SECRET_KEY").ok())
            .or(file_sk);

        Config {
            host,
            port,
            data_dir,
            access_key,
            secret_key,
        }
    }

    pub fn into_app_state(self) -> AppState {
        let auth_config = match (self.access_key, self.secret_key) {
            (Some(ak), Some(sk)) => Some(AuthConfig {
                access_key: ak,
                secret_key: sk,
            }),
            _ => None,
        };

        AppState {
            storage: FsStorage::new(&self.data_dir),
            auth_config,
        }
    }

    pub fn addr(&self) -> String {
        format!("{}:{}", self.host, self.port)
    }
}

fn load_file_config(explicit_path: Option<&std::path::Path>) -> FileConfig {
    let path = match explicit_path {
        Some(p) => p.to_path_buf(),
        None => PathBuf::from("devfs.toml"),
    };

    let content = match std::fs::read_to_string(&path) {
        Ok(c) => c,
        Err(_) => {
            if explicit_path.is_some() {
                tracing::warn!("config file not found: {}", path.display());
            }
            return FileConfig::default();
        }
    };

    match toml::from_str(&content) {
        Ok(config) => {
            tracing::info!("loaded config from {}", path.display());
            config
        }
        Err(e) => {
            tracing::warn!("failed to parse {}: {}", path.display(), e);
            FileConfig::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_config_deserialize_full() {
        let toml = r#"
host = "0.0.0.0"
port = 8080
data_dir = "/tmp/devfs"

[auth]
access_key = "mykey"
secret_key = "mysecret"
"#;
        let config: FileConfig = toml::from_str(toml).unwrap();
        assert_eq!(config.host.unwrap(), "0.0.0.0");
        assert_eq!(config.port.unwrap(), 8080);
        assert_eq!(config.data_dir.unwrap(), PathBuf::from("/tmp/devfs"));
        let auth = config.auth.unwrap();
        assert_eq!(auth.access_key.unwrap(), "mykey");
        assert_eq!(auth.secret_key.unwrap(), "mysecret");
    }

    #[test]
    fn file_config_deserialize_partial() {
        let toml = r#"
port = 3000
"#;
        let config: FileConfig = toml::from_str(toml).unwrap();
        assert!(config.host.is_none());
        assert_eq!(config.port.unwrap(), 3000);
        assert!(config.data_dir.is_none());
        assert!(config.auth.is_none());
    }

    #[test]
    fn file_config_deserialize_empty() {
        let config: FileConfig = toml::from_str("").unwrap();
        assert!(config.host.is_none());
        assert!(config.port.is_none());
        assert!(config.data_dir.is_none());
        assert!(config.auth.is_none());
    }

    #[test]
    fn load_file_config_missing_file() {
        let config = load_file_config(Some(std::path::Path::new("/nonexistent/devfs.toml")));
        assert!(config.host.is_none());
        assert!(config.port.is_none());
    }
}
