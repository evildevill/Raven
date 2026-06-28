use std::path::PathBuf;

use serde::Deserialize;
use tracing::{debug, info};

#[derive(Debug, Clone, Deserialize, Default)]
pub struct Config {
    pub proxy: Option<String>,
    pub timeout: Option<f64>,
    pub concurrency: Option<usize>,
    pub rate_limit: Option<f64>,
    pub retry: Option<usize>,
    pub nsfw: Option<bool>,
    pub no_color: Option<bool>,
    pub ignore_exclusions: Option<bool>,
    pub tor: Option<bool>,
    pub unique_tor: Option<bool>,
    pub dump_response: Option<bool>,
    pub profile: Option<bool>,
    pub deep: Option<bool>,
    pub avatar_match: Option<bool>,
}

impl Config {
    pub fn load() -> Self {
        let path = match config_path() {
            Some(p) => p,
            None => return Self::default(),
        };

        if !path.exists() {
            debug!("No config file at {}", path.display());
            return Self::default();
        }

        match std::fs::read_to_string(&path) {
            Ok(content) => {
                match toml::from_str(&content) {
                    Ok(config) => {
                        info!("Loaded config from {}", path.display());
                        config
                    }
                    Err(e) => {
                        debug!("Failed to parse config: {e}");
                        Self::default()
                    }
                }
            }
            Err(e) => {
                debug!("Failed to read config: {e}");
                Self::default()
            }
        }
    }
}

fn config_path() -> Option<PathBuf> {
    let proj_dirs = directories::ProjectDirs::from("", "", "raven")?;
    let mut path = proj_dirs.config_dir().to_path_buf();
    path.push("config.toml");
    Some(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_default() {
        let c = Config::default();
        assert!(c.proxy.is_none());
        assert!(c.timeout.is_none());
        assert!(c.tor.is_none());
        assert!(c.unique_tor.is_none());
    }

    #[test]
    fn config_toml_basic() {
        let toml = r#"
timeout = 30.0
concurrency = 10
proxy = "socks5://127.0.0.1:9050"
"#;
        let c: Config = toml::from_str(toml).unwrap();
        assert_eq!(c.timeout.unwrap(), 30.0);
        assert_eq!(c.concurrency.unwrap(), 10);
        assert_eq!(c.proxy.unwrap(), "socks5://127.0.0.1:9050");
    }

    #[test]
    fn config_toml_tor_options() {
        let toml = r#"
tor = true
unique_tor = true
no_color = true
nsfw = true
ignore_exclusions = true
"#;
        let c: Config = toml::from_str(toml).unwrap();
        assert!(c.tor.unwrap());
        assert!(c.unique_tor.unwrap());
        assert!(c.no_color.unwrap());
        assert!(c.nsfw.unwrap());
        assert!(c.ignore_exclusions.unwrap());
    }

    #[test]
    fn config_toml_partial() {
        let toml = r#"rate_limit = 15.5"#;
        let c: Config = toml::from_str(toml).unwrap();
        assert_eq!(c.rate_limit.unwrap(), 15.5);
        assert!(c.proxy.is_none());
    }

    #[test]
    fn config_toml_retry() {
        let toml = r#"retry = 3"#;
        let c: Config = toml::from_str(toml).unwrap();
        assert_eq!(c.retry.unwrap(), 3);
    }

    #[test]
    fn config_toml_dump_response() {
        let toml = r#"dump_response = true"#;
        let c: Config = toml::from_str(toml).unwrap();
        assert!(c.dump_response.unwrap());
    }

    #[test]
    fn config_toml_empty() {
        let c: Config = toml::from_str("").unwrap();
        assert!(c.proxy.is_none());
        assert!(c.timeout.is_none());
    }
}
