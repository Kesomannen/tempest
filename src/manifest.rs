use anyhow::anyhow;
use indexmap::IndexMap;
use loadsmith::{Dependency, PackageId, VersionReq};
use serde::{Deserialize, Serialize};

use crate::{Result, source::Source};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Manifest {
    pub profile: ProfileInfo,
    pub mods: Mods,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProfileInfo {
    pub game: String,
    #[serde(default)]
    pub default_source: Option<Source>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(transparent)]
pub struct Mods(IndexMap<PackageId, ModSpec>);

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ModSpec {
    Simple(VersionReq),
    Full {
        #[serde(default)]
        version: Option<VersionReq>,
        #[serde(default)]
        source: Option<Source>,
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
        Self {
            game: game.into(),
            default_source: None,
        }
    }

    pub fn with_default_source(mut self, source: Source) -> Self {
        self.default_source = Some(source);
        self
    }
}

impl Mods {
    pub fn new(mods: IndexMap<PackageId, ModSpec>) -> Self {
        Self(mods)
    }

    pub fn insert(&mut self, package_id: PackageId, mod_: ModSpec) -> Option<ModSpec> {
        self.0.insert(package_id, mod_)
    }

    pub fn get(&self, package_id: &PackageId) -> Option<&ModSpec> {
        self.0.get(package_id)
    }

    pub fn remove(&mut self, package_id: &PackageId) -> Option<ModSpec> {
        self.0.shift_remove(package_id)
    }

    pub fn into_dependencies(self, default_source: Source) -> impl Iterator<Item = Dependency> {
        self.0
            .into_iter()
            .map(move |(package_id, mod_)| mod_.into_dependency(package_id, default_source))
    }

    pub fn get_or_search<'a>(
        &'a self,
        name: &'a PackageId,
    ) -> Result<(&'a PackageId, &'a ModSpec)> {
        if let Some(mod_) = self.0.get(name) {
            Ok((name, mod_))
        } else {
            self.search_one(name.as_str())
        }
    }

    pub fn search_one(&self, query: &str) -> Result<(&PackageId, &ModSpec)> {
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

    pub fn search(&self, query: &str) -> impl Iterator<Item = (&PackageId, &ModSpec)> {
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

impl ModSpec {
    pub fn new(version_req: impl Into<VersionReq>) -> Self {
        Self::Simple(version_req.into())
    }

    pub fn with_source(self, source: Source) -> Self {
        match self {
            ModSpec::Simple(range) => Self::Full {
                version: Some(range),
                source: Some(source),
                registry_metadata: serde_json::Value::Null,
            },
            ModSpec::Full {
                version,
                registry_metadata,
                ..
            } => Self::Full {
                version,
                source: Some(source),
                registry_metadata,
            },
        }
    }

    pub fn with_registry_metadata(self, metadata: serde_json::Value) -> Self {
        match self {
            ModSpec::Simple(range) => Self::Full {
                version: Some(range),
                source: None,
                registry_metadata: metadata,
            },
            ModSpec::Full {
                version, source, ..
            } => Self::Full {
                version,
                source,
                registry_metadata: metadata,
            },
        }
    }

    fn into_dependency(self, package_id: PackageId, default_source: Source) -> Dependency {
        let guessed_source = self.guess_source();

        let (version_range, source, registry_metadata) = match self {
            ModSpec::Simple(range) => (range, None, None),
            ModSpec::Full {
                version,
                source,
                registry_metadata,
            } => (
                version.unwrap_or(VersionReq::STAR),
                source,
                match registry_metadata {
                    serde_json::Value::Object(map) if map.is_empty() => None,
                    serde_json::Value::Null => None,
                    _ => Some(registry_metadata),
                },
            ),
        };

        let source = source.or(guessed_source).unwrap_or(default_source);

        let mut dep = Dependency::new(package_id, version_range, source.to_string());
        if let Some(metadata) = registry_metadata {
            dep = dep.with_registry_metadata(metadata);
        }
        dep
    }

    fn guess_source(&self) -> Option<Source> {
        if let ModSpec::Full {
            registry_metadata: serde_json::Value::Object(map),
            ..
        } = self
        {
            if map.contains_key("path") {
                return Some(Source::Local);
            }

            if map.contains_key("repo") {
                return Some(Source::Github);
            }
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
        let mod_ = mods.0.shift_remove(&package_id).unwrap();

        let dependency = mod_.into_dependency(package_id, Source::Thunderstore);
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
        let mod_ = mods.0.shift_remove(&package_id).unwrap();

        let dependency = mod_.into_dependency(package_id, Source::Thunderstore);
        assert_eq!(dependency.source, "thunderstore");
        assert_eq!(dependency.registry_metadata, None);
    }
}
