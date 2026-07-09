use crate::{Context, Result};

#[derive(Debug, clap::Parser)]
#[command(about = "Update the Thunderstore package index for the current profile's game")]
pub struct FetchCommand {
    game: Option<String>,
}

impl super::Command for FetchCommand {
    async fn run(self, ctx: &Context) -> Result<()> {
        let game = self.game.map(Result::Ok).unwrap_or_else(|| {
            let profile = ctx.read_profile()?;
            Ok(profile.game().to_string())
        })?;

        crate::index::update(ctx, &game).await?;
        Ok(())
    }
}
