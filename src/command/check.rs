use tracing::info;

use crate::{Context, Result};

#[derive(Debug, clap::Parser)]
#[command(about = "Read the current profile and report any issues")]
pub struct CheckCommand;

impl super::Command for CheckCommand {
    async fn run(self, ctx: &Context) -> Result<()> {
        let _profile = super::read_profile(ctx)?;

        info!("all systems go!");

        Ok(())
    }
}
