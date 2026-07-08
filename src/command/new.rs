use std::path::PathBuf;

use anyhow::ensure;
use tracing::info;

use crate::{
    Context, Result,
    manifest::{Manifest, Mods, ProfileInfo},
    profile::Profile,
};

#[derive(Debug, clap::Parser)]
#[command(about = "Create a new profile", alias = "create")]
pub struct NewCommand {
    path: PathBuf,
    game: String,
}

impl super::Command for NewCommand {
    async fn run(self, _ctx: &Context) -> Result<()> {
        ensure!(
            !self.path.exists(),
            "path `{}` already exists",
            self.path.display()
        );

        let profile = Profile::create(
            self.path,
            Manifest::new(ProfileInfo::new(self.game), Mods::default()),
        )?;

        info!("created new profile at `{}`", profile.path().display());

        Ok(())
    }
}
