use anyhow::{Context as _, bail, ensure};
use loadsmith::{LocalRegistry, PackageId, VersionReq};
use tracing::{debug, info};

use crate::{Context, Result, index::Index, manifest::ModSpec, profile::Profile, source::Source};

#[derive(Debug, clap::Parser)]
#[command(about = "Add mods to the current profile", alias = "a")]
pub struct AddCommand {
    #[arg(
        help = "List of space-separated mods to add, optionally with version range (e.g. 'package@=1.2.3')"
    )]
    mods: Vec<String>,

    #[arg(short, long, help = "Specify the source for the added mods")]
    source: Option<Source>,

    #[arg(
        short,
        long,
        help = "Upgrade already installed mods to the latest version that satisfies the specified version range"
    )]
    upgrade: bool,
}

impl super::Command for AddCommand {
    async fn run(self, ctx: &Context) -> Result<()> {
        ensure!(!self.mods.is_empty(), "no mods specified");

        let mut profile = super::read_profile(ctx)?;
        ctx.indexes.prepare(ctx, profile.game()).await?;

        let mut resolved = Vec::new();

        let source = self.source.unwrap_or(profile.default_source());

        if source == Source::Local {
            let registry = LocalRegistry::default();
            for path in self.mods {
                let (package_id, version_info) = registry
                    .read(loadsmith::registry::local::Metadata::new(path.clone()))
                    .with_context(|| format!("failed to read local mod at path '{path}'"))?;

                let Some(package_id) = package_id else {
                    bail!("could not determine package ID for local mod at path '{path}'");
                };

                let version_req: VersionReq = version_info
                    .version
                    .to_string()
                    .parse()
                    .expect("version should always be a valid requirement");

                let spec = ModSpec::new(version_req).with_registry_metadata(serde_json::json!({
                    "path": path,
                }));
                resolved.push((package_id, spec));
            }
        } else {
            let Some(index) = ctx.indexes.get(source) else {
                bail!("no index found for source {source}");
            };

            for mod_ in &self.mods {
                if let Some((id, spec)) = self.resolve_from_index(mod_, &profile, index)? {
                    resolved.push((id, spec));
                };
            }
        }

        if resolved.is_empty() {
            debug!("no mods to add");
            return Ok(());
        }

        for (id, spec) in resolved {
            profile.manifest.mods.insert(id.clone(), spec);
        }

        profile.write_manifest()?;

        profile.resolve_and_sync(ctx, false).await?;

        Ok(())
    }
}

impl AddCommand {
    fn resolve_from_index(
        &self,
        s: &str,
        profile: &Profile,
        index: &Index,
    ) -> Result<Option<(PackageId, ModSpec)>> {
        let (name, version_req) = match s.split_once('@') {
            Some((id, version_str)) => {
                let version: VersionReq = version_str
                    .parse()
                    .with_context(|| format!("invalid version requirement: {version_str}"))?;
                (id, Some(version))
            }
            None => (s, None),
        };

        let id = PackageId::new(name);

        let (id, versions) = match index.version_info(&id)? {
            Some(versions) => (id, versions),
            None => {
                debug!("mod '{id}' not found in index, searching by name...");
                let results = index.search_packages(name, Some(profile.game()))?;

                let Some(id) = Self::pick_search_result(results, name)? else {
                    info!("search results denied, skipping mod '{name}'");
                    return Ok(None);
                };

                let versions = index
                    .version_info(&id)?
                    .expect("mod returned from search should have version info");

                (id, versions)
            }
        };

        if !self.upgrade
            && version_req
                .as_ref()
                .is_none_or(|req| *req == VersionReq::STAR)
            && profile.manifest.mods.get(&id).is_some()
        {
            debug!("mod {id} is already in the manifest at a compatible version, skipping");
            return Ok(None);
        }

        self.select_version(id, version_req, versions).map(Some)
    }

    fn select_version(
        &self,
        id: PackageId,
        version_req: Option<VersionReq>,
        versions: Vec<loadsmith::registry::VersionInfo>,
    ) -> Result<(PackageId, ModSpec)> {
        let version_range = if let Some(version) = version_req {
            let any_matches = versions.iter().any(|v| version.matches(&v.version));
            ensure!(
                any_matches,
                "no versions of {id} match the specified version range {version}"
            );

            version
        } else {
            let latest = versions
                .into_iter()
                .max_by(|a, b| a.version.cmp(&b.version))
                .expect("version info should not be empty");

            latest
                .version
                .to_string()
                .parse()
                .expect("version should always be a valid requirement")
        };

        let mut spec = ModSpec::new(version_range);
        if let Some(source) = self.source {
            spec = spec.with_source(source);
        }

        Ok((id, spec))
    }

    fn pick_search_result(mut results: Vec<PackageId>, name: &str) -> Result<Option<PackageId>> {
        match results.len() {
            0 => bail!("mod with name '{name}' not found"),
            1 => {
                let package_id = results.into_iter().next().unwrap();

                if package_id
                    .as_str()
                    .split_once('-')
                    .is_some_and(|(_author, package_name)| name == package_name)
                {
                    debug!("found mod with id {package_id} matching name '{name}'");
                    return Ok(Some(package_id));
                }

                let confirmed = dialoguer::Confirm::new()
                    .with_prompt(format!(
                        "found mod with id {package_id}, is this the mod you meant?",
                    ))
                    .default(true)
                    .interact()?;

                if !confirmed {
                    return Ok(None);
                }

                Ok(Some(package_id))
            }
            count if count <= 20 => {
                let selection = dialoguer::Select::new()
                                .with_prompt(format!(
                                    "mod with name '{name}' was not found, please select a mod from the search results",
                                ))
                                .items(&results)
                                .default(0)
                                .interact()?;

                Ok(Some(results.swap_remove(selection)))
            }
            count => bail!(
                "mod with name '{name}' was not found, and there are too many search results ({count}) to display, please refine your search query",
            ),
        }
    }
}
