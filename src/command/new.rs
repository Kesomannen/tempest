use std::path::PathBuf;

use anyhow::ensure;
use tracing::info;

use crate::{
    Context, Result,
    manifest::{Manifest, Mods, ProfileInfo},
    profile::Profile,
    schema::ThunderstoreSchema,
    source::Source,
};

#[derive(Debug, clap::Parser)]
#[command(about = "Create a new profile", alias = "create")]
pub struct NewCommand {
    path: PathBuf,
    game: String,
    default_source: Option<Source>,
}

impl super::Command for NewCommand {
    async fn run(self, ctx: &Context) -> Result<()> {
        ensure!(
            !self.path.exists(),
            "path `{}` already exists",
            self.path.display()
        );

        let schema = ThunderstoreSchema::load(ctx).await?;
        // check that the game and platform(s) are supported
        let _loader = schema.make_loader(&self.game)?;
        let _platforms = schema.make_platforms(&self.game)?;

        let mut profile_info = ProfileInfo::new(self.game);
        if let Some(source) = self.default_source {
            profile_info = profile_info.with_default_source(source);
        }

        let profile = Profile::create(self.path, Manifest::new(profile_info, Mods::default()))?;

        profile.write_git_ignore_from_schema(&schema)?;

        info!(
            "created new profile at `{}` with game {}{}",
            profile.path().display(),
            profile.game(),
            if let Some(source) = self.default_source {
                format!(" and default source {source}")
            } else {
                String::new()
            }
        );

        Ok(())
    }
}
