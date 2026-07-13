use std::path::PathBuf;

use anyhow::ensure;
use tracing::info;

use crate::{
    Context, Result,
    manifest::{Manifest, Mods, ProfileInfo},
    profile::Profile,
    schema::ThunderstoreSchema,
};

#[derive(Debug, clap::Parser)]
#[command(about = "Create a new profile", alias = "create")]
pub struct NewCommand {
    path: PathBuf,
    game: String,
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

        let profile = Profile::create(
            self.path,
            Manifest::new(ProfileInfo::new(self.game), Mods::default()),
        )?;

        info!(
            "created new profile at `{}` with game {}",
            profile.path().display(),
            profile.game()
        );

        Ok(())
    }
}
