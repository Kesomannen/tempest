use std::{collections::BTreeMap, io::Cursor, path::PathBuf};

use anyhow::{Context as _, bail};
use loadsmith::{PackageId, VersionRange, r2z, thunderstore::PackageIdExt};
use tracing::{debug, info, warn};
use tracing_indicatif::{span_ext::IndicatifSpanExt, style::ProgressStyle};
use uuid::Uuid;

use crate::{
    Context, Result,
    manifest::{self, Manifest, Mods, ProfileInfo},
    profile::Profile,
};

#[derive(Debug, clap::ValueEnum, Clone, Copy, PartialEq, Eq, Default)]
enum Type {
    #[default]
    Thunderstore,
    Local,
    Gale,
}

#[derive(Debug, clap::Parser)]
#[command(about = "Import a profile code from Thunderstore")]
pub struct ImportCommand {
    #[arg(help = "Gale sync profile code, Thunderstore profile UUID, or file path to import from")]
    source: String,

    #[arg(help = "Path to save the imported profile")]
    path: Option<PathBuf>,

    #[arg(long)]
    game: Option<String>,

    #[arg(short, long)]
    merge: bool,

    #[arg(
        short,
        long,
        help = "Source to import from. If omitted, it will be guessed based on the source argument"
    )]
    type_: Option<Type>,
}

impl super::Command for ImportCommand {
    async fn run(self, ctx: &Context) -> Result<()> {
        let content = self.read_code(ctx).await?;

        let mut import_file = r2z::ImportFile::open(Cursor::new(content))?;
        let import_manifest = import_file
            .read_manifest::<crate::ExtraImportData>()
            .context("failed to read manifest")?;

        let game = self
            .game
            .or_else(|| import_manifest.extra.community.clone())
            .unwrap_or_else(|| {
                warn!("imported profile does not specify a game, defaulting to 'unknown'");

                "unknown".to_string()
            });

        let mods = import_manifest
            .mods
            .into_iter()
            .map(|m| {
                (
                    PackageId::from_ts_ident(m.name),
                    manifest::Mod::new(VersionRange::exact(m.version)),
                )
            })
            .collect::<BTreeMap<_, _>>();

        let path = Self::resolve_path(ctx, self.path, &import_manifest.profile_name);

        let mut existing_profile: Option<Profile> = None;

        if path.exists() {
            if !path.is_dir() {
                bail!("target path `{}` is not a directory", path.display());
            }

            if Profile::is_profile_dir(&path) {
                existing_profile = Some(Profile::read(&path)?);
            } else {
                let confirmed = dialoguer::Confirm::new()
                    .with_prompt(format!(
                        "directory `{}` already exists, do you want to overwrite it?",
                        path.display()
                    ))
                    .default(false)
                    .interact()?;

                if !confirmed {
                    bail!("aborted");
                }

                std::fs::remove_dir_all(&path)?;
            }
        }

        let mut profile = match existing_profile {
            Some(mut existing) => {
                Self::handle_existing(&mut existing, self.merge, mods)?;

                if game != existing.game() {
                    warn!(
                        "imported profile game '{}' does not match existing profile game '{}'",
                        game,
                        existing.game()
                    );
                }

                existing
            }
            None => {
                if self.merge {
                    warn!(
                        "merge option specified but no existing profile found, creating new profile"
                    );
                }

                let manifest = Manifest::new(ProfileInfo::new(game), Mods::new(mods));

                Profile::create(path, manifest)?
            }
        };

        profile.resolve_and_sync(ctx, false).await?;

        import_file
            .import_config_files(profile.path(), true)
            .context("failed to import config files")?;

        info!("imported profile to `{}`", profile.path().display());

        Ok(())
    }
}

impl ImportCommand {
    fn handle_existing(
        existing: &mut Profile,
        merge: bool,
        mods: BTreeMap<PackageId, manifest::Mod>,
    ) -> Result {
        if merge {
            debug!("merging with existing profile");
            for (id, mod_) in mods {
                existing.manifest.mods.insert(id, mod_);
            }
        } else {
            let confirmed = dialoguer::Confirm::new()
                .with_prompt(format!(
                    "profile already exists at `{}`, do you want to overwrite it?",
                    existing.path().display()
                ))
                .default(true)
                .interact()?;

            if !confirmed {
                bail!("aborted");
            }

            info!("overwriting existing profile");
            existing.manifest.mods = Mods::new(mods);
        }

        existing.write_manifest()?;

        Ok(())
    }

    fn resolve_path(ctx: &Context, path_option: Option<PathBuf>, import_name: &str) -> PathBuf {
        match path_option {
            Some(path) => path,
            None if Profile::is_profile_dir(&ctx.working_dir) => {
                debug!("current working directory is a profile, importing into it");

                ctx.working_dir.clone()
            }
            None => {
                debug!("no path specified, importing into a new profile directory");

                ctx.working_dir.join(import_name)
            }
        }
    }

    async fn read_code(&self, ctx: &Context) -> Result<Vec<u8>> {
        let type_ = self
            .type_
            .or_else(|| Type::guess(&self.source))
            .unwrap_or_default();

        match type_ {
            Type::Thunderstore => {
                let uuid: Uuid = self.source.parse().context("source is not a valid UUID")?;

                info!("importing profile from Thunderstore with code {uuid}");

                let span = tracing::info_span!("import", %uuid);
                span.pb_set_style(&ProgressStyle::default_spinner());
                span.pb_set_message("fetching profile from Thunderstore...");

                let _enter = span.enter();

                let content = ctx
                    .thunderstore
                    .get_profile(uuid)
                    .await
                    .context("error downloading profile")?;
                Ok(content)
            }
            Type::Local => {
                info!("importing profile from local file `{}`", self.source);

                let path = PathBuf::from(&self.source);

                if !path.exists() {
                    bail!("file `{}` does not exist", path.display());
                }

                let content = std::fs::read(&path)?;
                Ok(content)
            }
            Type::Gale => {
                info!("importing Gale sync profile with code {}", self.source);

                let span = tracing::info_span!("import", %self.source);
                span.pb_set_style(&ProgressStyle::default_spinner());
                span.pb_set_message("fetching profile from Gale...");

                let _enter = span.enter();

                let url = format!("https://gale.kesomannen.com/api/profile/{}", self.source);
                let content = ctx
                    .http
                    .get(&url)
                    .send()
                    .await
                    .and_then(|response| response.error_for_status())
                    .context("error downloading profile")?
                    .bytes()
                    .await?
                    .to_vec();

                Ok(content)
            }
        }
    }
}

impl Type {
    fn guess(source: &str) -> Option<Self> {
        if Uuid::parse_str(source).is_ok() {
            Some(Type::Thunderstore)
        } else if PathBuf::from(source).exists() {
            Some(Type::Local)
        } else if source.chars().all(|c| c.is_alphanumeric()) {
            Some(Type::Gale)
        } else {
            None
        }
    }
}
