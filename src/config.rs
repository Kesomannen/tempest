use std::{path::Path, str::FromStr, time::Duration};

use anyhow::{Context as _, bail};
use serde::{Deserialize, Serialize};
use tracing::debug;

use crate::{Context, Result};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
#[derive(Default)]
pub struct Config {
    pub auto_fetch: AutoFetch,
}

impl Config {
    const FILE_NAME: &'static str = "config.toml";

    pub fn read(home: impl AsRef<Path>) -> Result<Option<Self>> {
        let path = home.as_ref().join(Self::FILE_NAME);
        if !path.exists() {
            return Ok(None);
        }

        debug!("reading config from `{}`", path.display());

        let s = std::fs::read_to_string(&path).context("error reading config")?;
        let config = toml::from_str(&s).context("failed to parse config")?;

        Ok(Some(config))
    }

    pub fn write(&self, ctx: &Context) -> Result<()> {
        let path = ctx.home_dir.join(Self::FILE_NAME);

        let s = toml::to_string_pretty(self).context("failed to serialize config")?;
        std::fs::write(&path, s).context("error writing config")?;

        Ok(())
    }

    pub(crate) fn set(&mut self, property: &str, value: &str) -> Result<()> {
        match property {
            "auto_fetch.enabled" => {
                self.auto_fetch.enabled = value.parse()?;
            }
            "auto_fetch.interval" => {
                self.auto_fetch.interval = value.parse()?;
            }
            _ => bail!("unknown config property `{property}`"),
        }

        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AutoFetch {
    pub enabled: bool,
    pub interval: HumanDuration,
}

impl Default for AutoFetch {
    fn default() -> Self {
        Self {
            enabled: false,
            interval: HumanDuration(std::time::Duration::from_hours(24)), // 1 day
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(into = "String", try_from = "String")]
pub struct HumanDuration(pub Duration);

impl From<HumanDuration> for String {
    fn from(value: HumanDuration) -> Self {
        humantime::format_duration(value.0).to_string()
    }
}

impl TryFrom<String> for HumanDuration {
    type Error = humantime::DurationError;

    fn try_from(value: String) -> std::result::Result<Self, Self::Error> {
        value.parse()
    }
}

impl FromStr for HumanDuration {
    type Err = humantime::DurationError;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        Ok(Self(humantime::parse_duration(s)?))
    }
}
