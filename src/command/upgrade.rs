use crate::{Context, Result};

#[derive(Debug, clap::Parser)]
#[command(about = "Update mods for the current profile")]
pub struct UpgradeCommand;

impl super::Command for UpgradeCommand {
    async fn run(self, ctx: &Context) -> Result<()> {
        let mut profile = ctx.read_profile()?;
        ctx.indexes.prepare(ctx, profile.game()).await?;

        profile.resolve_and_sync(ctx, true).await?;

        Ok(())
    }
}
