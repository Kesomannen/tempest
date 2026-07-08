use std::{fs, path::Path};

use anyhow::Context as _;
use serde::de::DeserializeOwned;

use crate::Result;

pub fn read<T: DeserializeOwned, F>(path: impl AsRef<Path>, parse: F) -> Result<T>
where
    F: FnOnce(&str) -> Result<T>,
{
    let content = fs::read_to_string(path)?;
    let value = parse(&content)?;
    Ok(value)
}

pub fn write(path: impl AsRef<Path>, content: &str) -> Result {
    let path = path.as_ref();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    fs::write(path, content)?;
    Ok(())
}

pub fn read_toml<T: DeserializeOwned>(path: impl AsRef<Path>) -> Result<T> {
    read(path, |content| {
        toml::from_str(content).context("failed to parse TOML")
    })
}

pub fn write_toml<T: serde::Serialize>(path: impl AsRef<Path>, value: &T) -> Result {
    write(path, &toml::to_string_pretty(value)?)
}

pub fn read_json<T: DeserializeOwned>(path: impl AsRef<Path>) -> Result<T> {
    read(path, |content| {
        serde_json::from_str(content).context("failed to parse JSON")
    })
}

pub fn write_json<T: serde::Serialize>(path: impl AsRef<Path>, value: &T) -> Result {
    write(path, &serde_json::to_string_pretty(value)?)
}
