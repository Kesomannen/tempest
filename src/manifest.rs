use std::collections::BTreeMap;

use anyhow::anyhow;
use loadsmith::core::{Dependency, PackageId, VersionRange};
use serde::{Deserialize, Serialize};

use crate::Result;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Manifest {
    pub profile: ProfileInfo,
    pub mods: Mods,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProfileInfo {
    pub game: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Mods(BTreeMap<PackageId, Mod>);

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Mod {
    Simple(VersionRange),
    Full {
        version: VersionRange,
        #[serde(default)]
        source: Option<String>,
        #[serde(flatten)]
        registry_metadata: serde_json::Value,
    },
}

impl Manifest {
    pub fn new(profile: ProfileInfo, mods: Mods) -> Self {
        Self { profile, mods }
    }
}

impl ProfileInfo {
    pub fn new(game: impl Into<String>) -> Self {
        Self { game: game.into() }
    }
}

impl Mods {
    pub fn new(mods: BTreeMap<PackageId, Mod>) -> Self {
        Self(mods)
    }

    pub fn insert(&mut self, package_id: PackageId, mod_: Mod) {
        self.0.insert(package_id, mod_);
    }

    pub fn get(&self, package_id: &PackageId) -> Option<&Mod> {
        self.0.get(package_id)
    }

    pub fn remove(&mut self, package_id: &PackageId) -> Option<Mod> {
        self.0.remove(package_id)
    }

    pub fn into_dependencies(self) -> impl Iterator<Item = Dependency> {
        self.0
            .into_iter()
            .map(|(package_id, mod_)| mod_.into_dependency(package_id))
    }

    pub fn get_or_search<'a>(&'a self, name: &'a PackageId) -> Result<(&'a PackageId, &'a Mod)> {
        if let Some(mod_) = self.0.get(name) {
            Ok((name, mod_))
        } else {
            self.search_one(name.as_str())
        }
    }

    pub fn search_one(&self, query: &str) -> Result<(&PackageId, &Mod)> {
        let mut results = self.search(query);

        match results.next() {
            Some(first) => {
                if results.next().is_some() {
                    Err(anyhow!("multiple mods found matching '{query}'"))
                } else {
                    Ok(first)
                }
            }
            None => Err(anyhow!("no mods found matching '{query}'")),
        }
    }

    pub fn search(&self, query: &str) -> impl Iterator<Item = (&PackageId, &Mod)> {
        let lower_query = query.to_lowercase();

        self.0.iter().filter(move |(package_id, _)| {
            package_id
                .as_str()
                .to_lowercase()
                .split('-')
                .any(|segment| segment == lower_query)
        })
    }
}

impl Mod {
    const DEFAULT_REGISTRY: &str = "thunderstore";
    const LOCAL_REGISTRY: &str = "local";

    pub fn new(version: impl Into<VersionRange>) -> Self {
        Self::Simple(version.into())
    }

    pub fn with_source(self, source: impl Into<String>) -> Self {
        match self {
            Mod::Simple(range) => Self::Full {
                version: range,
                source: Some(source.into()),
                registry_metadata: serde_json::Value::Null,
            },
            Mod::Full {
                version,
                registry_metadata,
                ..
            } => Self::Full {
                version,
                source: Some(source.into()),
                registry_metadata,
            },
        }
    }

    // pub fn version_range(&self) -> &VersionRange {
    //     match self {
    //         Mod::Simple(range) => range,
    //         Mod::Full { version, .. } => version,
    //     }
    // }

    fn into_dependency(self, package_id: PackageId) -> Dependency {
        let guessed_source = self.guess_source();

        let (version_range, source, registry_metadata) = match self {
            Mod::Simple(range) => (range, None, None),
            Mod::Full {
                version,
                source,
                registry_metadata,
            } => (
                version,
                source,
                match registry_metadata {
                    serde_json::Value::Object(map) if map.is_empty() => None,
                    _ => Some(registry_metadata),
                },
            ),
        };

        let source = source
            .or(guessed_source.map(|s| s.to_string()))
            .unwrap_or_else(|| Self::DEFAULT_REGISTRY.to_string());

        let mut dep = Dependency::new(package_id, version_range, source);
        if let Some(metadata) = registry_metadata {
            dep = dep.with_registry_metadata(metadata)
        }
        dep
    }

    fn guess_source(&self) -> Option<&'static str> {
        if let Mod::Full {
            registry_metadata: serde_json::Value::Object(map),
            ..
        } = self
            && map.contains_key("path")
        {
            return Some(Self::LOCAL_REGISTRY);
        }

        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deserialize_local_mod() {
        let package_id = PackageId::new("BepInEx-BepInExPack");

        let toml_str = r#"
            [BepInEx-BepInExPack]
            version = "*"
            path = ".."
        "#;

        let mut mods: Mods = toml::from_str(toml_str).unwrap();
        let mod_ = mods.0.remove(&package_id).unwrap();

        let dependency = mod_.into_dependency(package_id);
        assert_eq!(dependency.source, "local");
        assert_eq!(
            dependency.registry_metadata,
            Some(serde_json::json!({"path": ".."}))
        );
    }

    #[test]
    fn deserialize_mod() {
        let package_id = PackageId::new("BepInEx-BepInExPack");

        let toml_str = r#"
            [BepInEx-BepInExPack]
            version = "*"
        "#;

        let mut mods: Mods = toml::from_str(toml_str).unwrap();
        let mod_ = mods.0.remove(&package_id).unwrap();

        let dependency = mod_.into_dependency(package_id);
        assert_eq!(dependency.source, "thunderstore");
        assert_eq!(dependency.registry_metadata, None);
    }
}
