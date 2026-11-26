use std::env;
use std::fs;
use std::path::PathBuf;

use serde::Deserialize;

use crate::sources::Source;

#[derive(Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub source: Option<String>,
    pub lang: Option<String>,
    pub wpm: Option<u64>,
}

impl Config {
    pub fn source(&self) -> Result<Option<Source>, String> {
        self.source
            .as_deref()
            .map(|name| {
                Source::parse(name).ok_or_else(|| {
                    format!("unknown source '{name}' in {}", config_path().display())
                })
            })
            .transpose()
    }

    fn validate(&self) -> Result<(), String> {
        if self.wpm == Some(0) {
            return Err(format!(
                "wpm in {} must be greater than 0",
                config_path().display()
            ));
        }
        if self.lang.as_deref().is_some_and(str::is_empty) {
            return Err(format!(
                "lang in {} must not be empty",
                config_path().display()
            ));
        }
        self.source()?;
        Ok(())
    }
}

pub fn load() -> Result<Config, String> {
    let path = config_path();
    let text = match fs::read_to_string(&path) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(Config::default());
        }
        Err(error) => return Err(format!("could not read {}: {error}", path.display())),
    };
    let config: Config = toml::from_str(&text)
        .map_err(|error| format!("could not parse {}: {error}", path.display()))?;
    config.validate()?;
    Ok(config)
}

pub fn config_path() -> PathBuf {
    config_home()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("wiki")
        .join("config.toml")
}

fn config_home() -> Option<PathBuf> {
    if let Some(path) = env::var_os("XDG_CONFIG_HOME")
        && !path.is_empty()
    {
        return Some(PathBuf::from(path));
    }
    #[cfg(windows)]
    if let Some(path) = env::var_os("APPDATA")
        && !path.is_empty()
    {
        return Some(PathBuf::from(path));
    }
    env::var_os("HOME")
        .or_else(|| env::var_os("USERPROFILE"))
        .filter(|path| !path.is_empty())
        .map(PathBuf::from)
        .map(|home| home.join(".config"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_valid_defaults() {
        let config: Config =
            toml::from_str("source = \"itsfoss\"\nlang = \"tr\"\nwpm = 300").unwrap();
        config.validate().unwrap();
        assert_eq!(config.source().unwrap(), Some(Source::ItsFoss));
        assert_eq!(config.lang.as_deref(), Some("tr"));
        assert_eq!(config.wpm, Some(300));
    }

    #[test]
    fn rejects_invalid_and_unknown_settings() {
        for source in ["wpm = 0", "lang = \"\"", "source = \"example\""] {
            let config: Config = toml::from_str(source).unwrap();
            assert!(config.validate().is_err(), "{source}");
        }
        assert!(toml::from_str::<Config>("wpmm = 300").is_err());
    }

    #[test]
    fn locates_the_standard_config_path() {
        assert!(config_path().ends_with("wiki/config.toml"));
    }
}
