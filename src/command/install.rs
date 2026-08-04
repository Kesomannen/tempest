use crate::{Context, Result};

#[derive(Debug, clap::Parser)]
#[command(
    about = "Download and install mods for the current profile",
    alias = "i"
)]
pub struct InstallCommand;

impl super::Command for InstallCommand {
    async fn run(self, ctx: &Context) -> Result<()> {
        let mut profile = ctx.read_profile()?;
        ctx.indexes.prepare(ctx, profile.game()).await?;

        profile.resolve_and_sync(ctx, false).await?;

        Ok(())
    }
}
