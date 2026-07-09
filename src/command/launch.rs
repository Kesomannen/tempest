use std::process::{Command, Stdio};

use anyhow::Context as _;
use colored::Colorize;
use loadsmith::{Loader, Platform};
use tokio::io::{AsyncBufReadExt, AsyncRead};
use tracing::{debug, info, warn};
use tracing_indicatif::{span_ext::IndicatifSpanExt, style::ProgressStyle};

use crate::{Context, Result, profile::Profile, schema::ThunderstoreSchema};

#[derive(Debug, clap::Parser)]
#[command(about = "Launch the game with the current profile", alias = "run")]
pub struct LaunchCommand {
    #[arg(long, help = "Perform a dry run without launching the game")]
    pub dry_run: bool,
}

impl super::Command for LaunchCommand {
    async fn run(self, ctx: &Context) -> Result<()> {
        let profile = ctx.read_profile()?;
        let schema = ThunderstoreSchema::fetch(ctx).await?;

        let game = schema.game(profile.game())?;
        let loader = schema.make_loader(profile.game())?;
        let platform = schema.make_platform(profile.game())?;

        let game_name = &game.meta.display_name;

        info!("launching {} with {}", game_name, platform.name());

        let mut command = make_command(&profile, &*loader, &platform)?;

        debug!(?command, "launch command built");

        if self.dry_run {
            info!("dry run complete");
            return Ok(());
        }

        command.stdout(Stdio::piped());
        command.stderr(Stdio::piped());

        let span = tracing::info_span!("launch", %game_name);
        span.pb_set_style(&ProgressStyle::default_spinner());
        span.pb_set_message("waiting for game process to exit...");
        let enter = span.enter();

        let mut command = tokio::process::Command::from(command);
        let mut child = command.spawn()?;

        tokio::try_join!(
            log_stdio(child.stdout.take().unwrap(), "stdout"),
            log_stdio(child.stderr.take().unwrap(), "stderr"),
        )?;

        let status = child.wait().await?;

        drop(enter);

        if status.success() {
            info!("game process exited successfully");
        } else {
            warn!(
                %status,
                "game process exited with error"
            );
        }

        Ok(())
    }
}

fn make_command(profile: &Profile, loader: &dyn Loader, platform: &Platform) -> Result<Command> {
    let launch_ctx = platform
        .create_launch_context(profile.path_utf8(), None)
        .context("failed to create launch context")?;

    let mut command = platform
        .create_launch_command()?
        .context("launch method could not be determined")?;

    let args = loader
        .generate_launch_args(&launch_ctx)
        .context("error generating launch arguments")?;

    args.apply(&mut command);
    loader
        .prepare_launch(&launch_ctx)
        .context("error preparing for launch")?;
    Ok(command)
}

async fn log_stdio<R: AsyncRead + Unpin>(reader: R, prefix: &str) -> Result {
    let mut lines = tokio::io::BufReader::new(reader).lines();

    while let Some(line) = lines.next_line().await? {
        let s = format!("[{prefix}] {line}").black();
        debug!("{s}");
    }

    Ok(())
}
