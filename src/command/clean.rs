use anyhow::bail;
use loadsmith::PackageStoreEntry;
use tracing::{debug, info};
use tracing_indicatif::{span_ext::IndicatifSpanExt, style::ProgressStyle};

use crate::{Context, Result};

#[derive(Debug, clap::Parser)]
#[command(about = "Clean up mods from the disk store")]
pub struct CleanCommand {
    #[arg(
        long,
        help = "Remove all mods from the store, even if they are still referenced by profiles"
    )]
    force: bool,
}

impl super::Command for CleanCommand {
    async fn run(self, ctx: &Context) -> Result<()> {
        let to_remove: Vec<PackageStoreEntry> = if self.force {
            ctx.store.entries().collect::<Result<Vec<_>, _>>()?
        } else {
            ctx.store.unused_entries().collect::<Result<Vec<_>, _>>()?
        };

        if to_remove.is_empty() {
            info!("no mods to remove from store");
            return Ok(());
        }

        let confirm = dialoguer::Confirm::new()
            .with_prompt(format!(
                "Are you sure you want to remove {} mods from the store?",
                to_remove.len()
            ))
            .default(!self.force)
            .interact()?;

        if !confirm {
            bail!("aborted by user");
        }

        let span = tracing::info_span!("clean_store");
        span.pb_set_style(&ProgressStyle::default_bar());
        span.pb_set_message("removing mods...");
        span.pb_set_length(to_remove.len() as u64);
        let _enter = span.enter();

        for entry in to_remove {
            debug!("removing store entry {entry}");

            let store = ctx.store.clone();

            tokio::task::spawn_blocking(move || store.remove(&entry)).await??;

            span.pb_inc(1);
        }

        Ok(())
    }
}
