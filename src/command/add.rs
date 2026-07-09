use anyhow::{Context as _, bail, ensure};
use loadsmith::{PackageId, VersionRange};
use tracing::{debug, info};

use crate::{Context, Result, index, profile::Profile};

#[derive(Debug, clap::Parser)]
#[command(about = "Add packages to the current profile", alias = "a")]
pub struct AddCommand {
    #[arg(help = "List of packages to add, optionally with version range (e.g. 'package@=1.2.3')")]
    packages: Vec<String>,

    #[arg(short, long, help = "Specify the source for the added packages")]
    source: Option<String>,

    #[arg(
        short,
        long,
        help = "Upgrade existing packages to the latest version if available"
    )]
    upgrade: bool,
}

impl super::Command for AddCommand {
    async fn run(self, ctx: &Context) -> Result<()> {
        let mut profile = super::read_profile(ctx)?;
        index::check(ctx, profile.game()).await?;

        ensure!(!self.packages.is_empty(), "no packages specified");

        let mut resolved = Vec::new();
        for package in &self.packages {
            let Some((id, version_range)) = self.resolve(package, ctx, &profile)? else {
                continue;
            };

            resolved.push((id, version_range));
        }

        if resolved.is_empty() {
            debug!("no packages to add");
            return Ok(());
        }

        for (id, version_range) in resolved {
            let mut mod_ = crate::manifest::Mod::new(version_range);
            if let Some(source) = &self.source {
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
        ctx: &Context,
        profile: &Profile,
    ) -> Result<Option<(PackageId, VersionRange)>> {
        let (name, version) = match s.split_once('@') {
            Some((id, version_str)) => {
                let version: VersionRange = version_str
                    .parse()
                    .with_context(|| format!("invalid version range: {version_str}"))?;
                (id, Some(version))
            }
            None => (s, None),
        };

        let id = PackageId::new(name);

        let (id, versions) = match ctx.index.version_info(&id)? {
            Some(versions) => (id, versions),
            None => {
                debug!("package '{id}' not found in index, searching by name...");
                let results = ctx.index.search_packages(name, Some(profile.game()))?;

                let Some(id) = Self::pick_search_result(results, name)? else {
                    info!("search results denied, skipping package '{name}'");
                    return Ok(None);
                };

                let versions = ctx
                    .index
                    .version_info(&id)?
                    .expect("package returned from search should have version info");

                (id, versions)
            }
        };

        if !self.upgrade
            && version.as_ref().is_none_or(VersionRange::is_any)
            && profile.manifest.mods.get(&id).is_some()
        {
            debug!("package {id} is already in the manifest at a compatible version, skipping");
            return Ok(None);
        }

        let version_range = if let Some(version) = version {
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

            VersionRange::Exact(latest.version)
        };

        Ok(Some((id, version_range)))
    }

    fn pick_search_result(mut results: Vec<PackageId>, name: &str) -> Result<Option<PackageId>> {
        match results.len() {
            0 => bail!("package with name '{name}' not found"),
            1 => {
                let result = results.into_iter().next().unwrap();

                let confirmed = dialoguer::Confirm::new()
                    .with_prompt(format!(
                        "package with name '{name}' was not found, did you mean {result}?",
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
                                    "package with name '{name}' was not found, please select a package from the search results",
                                ))
                                .items(&results)
                                .default(0)
                                .interact()?;

                Ok(Some(results.swap_remove(selection)))
            }
            count => bail!(
                "package with name '{name}' was not found, and there are too many search results ({count}) to display, please refine your search query",
            ),
        }
    }
}
