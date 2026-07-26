use anyhow::{Context as _, bail, ensure};
use loadsmith::{PackageId, VersionReq};
use tracing::{debug, info};

use crate::{Context, Result, index::Index, profile::Profile, source::Source};

#[derive(Debug, clap::Parser)]
#[command(about = "Add mods to the current profile", alias = "a")]
pub struct AddCommand {
    #[arg(help = "List of mods to add, optionally with version range (e.g. 'package@=1.2.3')")]
    mods: Vec<String>,

    #[arg(short, long, help = "Specify the source for the added mods")]
    source: Option<Source>,

    #[arg(
        short,
        long,
        help = "Upgrade existing mods to the latest version if available"
    )]
    upgrade: bool,
}

impl super::Command for AddCommand {
    async fn run(self, ctx: &Context) -> Result<()> {
        ensure!(!self.mods.is_empty(), "no mods specified");

        let mut profile = super::read_profile(ctx)?;
        ctx.indexes.prepare(ctx, profile.game()).await?;

        let source = self.source.unwrap_or(profile.default_source());

        let Some(index) = ctx.indexes.get(source) else {
            bail!("no index found for source {source}");
        };

        let mut resolved = Vec::new();
        for mod_ in &self.mods {
            let Some((id, version_range)) = self.resolve(mod_, &profile, index)? else {
                continue;
            };

            resolved.push((id, version_range));
        }

        if resolved.is_empty() {
            debug!("no mods to add");
            return Ok(());
        }

        for (id, version_range) in resolved {
            let mut mod_ = crate::manifest::ModSpec::new(version_range);
            if let Some(source) = self.source {
                mod_ = mod_.with_source(source);
            }

            profile.manifest.mods.insert(id.clone(), mod_);
        }

        profile.write_manifest()?;

        profile.resolve_and_sync(ctx, false).await?;

        Ok(())
    }
}

impl AddCommand {
    fn resolve(
        &self,
        s: &str,
        profile: &Profile,
        index: &Index,
    ) -> Result<Option<(PackageId, VersionReq)>> {
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

        Ok(Some((id, version_range)))
    }

    fn pick_search_result(mut results: Vec<PackageId>, name: &str) -> Result<Option<PackageId>> {
        match results.len() {
            0 => bail!("mod with name '{name}' not found"),
            1 => {
                let result = results.into_iter().next().unwrap();

                let confirmed = dialoguer::Confirm::new()
                    .with_prompt(format!(
                        "found mod with id {result}, is this the mod you meant?",
                    ))
                    .default(true)
                    .interact()?;

                if !confirmed {
                    return Ok(None);
                }

                Ok(Some(result))
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
