use anyhow::Context as _;

use crate::{Context, Result};

#[derive(Debug, clap::Parser)]
#[command(about = "Update the mod index for the current game")]
pub struct FetchCommand {
    #[arg(
        short,
        long,
        help = "Specify the game to fetch the index for, or omit to use the game from the current profile"
    )]
    game: Option<String>,
}

impl super::Command for FetchCommand {
    async fn run(self, ctx: &Context) -> Result<()> {
        let game = match self.game {
            Some(game) => game,
            None => {
                let profile = ctx
                    .read_profile()
                    .context("failed to determine game to fetch")?;
                profile.game().to_string()
            }
        };

        ctx.indexes.update(&game).await?;
        Ok(())
    }
}
