use std::{
    io::{BufWriter, Cursor, Seek, Write},
    path::PathBuf,
};

use anyhow::{Context as _, bail};
use loadsmith::{r2z, thunderstore::PackageIdExt};
use tracing::{debug, info, warn};
use tracing_indicatif::span_ext::IndicatifSpanExt;

use crate::{Context, Result, Source, profile::Profile};

#[derive(Debug, clap::Parser)]
#[command(about = "Export the current profile to a file or Thunderstore code")]
pub struct ExportCommand {
    #[arg(help = "Path to export the profile to, or omit to upload to Thunderstore")]
    path: Option<PathBuf>,
}

impl super::Command for ExportCommand {
    async fn run(self, ctx: &Context) -> Result<()> {
        let profile = ctx.read_profile()?;

        if let Some(mut path) = self.path {
            if path.is_dir() {
                debug!(
                    "path `{}` is a directory, appending profile name",
                    path.display()
                );
                path = path.join(format!("{}.r2z", profile.name()));
            }

            if path.exists() {
                let confirmed = dialoguer::Confirm::new()
                    .with_prompt(format!(
                        "file `{}` already exists, do you want to overwrite it?",
                        path.display()
                    ))
                    .default(false)
                    .interact()?;

                if !confirmed {
                    bail!("aborted export");
                }
            }

            let mut file = std::fs::File::create(&path).map(BufWriter::new)?;
            Self::export_to(&profile, &mut file)?;

            info!("exported profile to `{}`", path.display());
        } else {
            let mut buffer = Vec::new();
            Self::export_to(&profile, Cursor::new(&mut buffer))?;

            let span = tracing::info_span!("upload_profile");
            span.pb_set_style(&tracing_indicatif::style::ProgressStyle::default_spinner());
            span.pb_set_message("uploading profile to Thunderstore...");
            let enter = span.enter();

            let key = ctx
                .thunderstore_client
                .create_profile(&buffer)
                .await
                .context("error while uploading profile")?;

            drop(enter);

            info!("uploaded profile to Thunderstore with key: {key}");
        }

        Ok(())
    }
}

impl ExportCommand {
    fn export_to<W: Write + Seek>(profile: &Profile, writer: W) -> Result {
        let mods = profile
            .lockfile
            .packages()
            .iter()
            .filter(|package| {
                if package.source == Source::Thunderstore.to_string() {
                    true
                } else {
                    warn!(
                        "excluding non-thunderstore package {} from export",
                        package.ref_.id()
                    );
                    false
                }
            })
            .map(|package| {
                let ident = package.ref_.id().clone().into_ts_ident()?;
                let version = package.ref_.version().clone();

                Ok(r2z::Mod::new(ident, version, true))
            })
            .collect::<Result<Vec<_>>>()?;

        let extra_data = crate::ExtraImportData {
            community: Some(profile.game().to_string()),
        };
        let manifest = r2z::ProfileManifest::new(profile.name(), mods, extra_data);

        let mut export = r2z::ExportFile::create(writer, &manifest)?;

        export
            .write_config_from_dir(profile.path_utf8(), true)
            .context("failed to export config")?;

        export.finish()?;

        Ok(())
    }
}
