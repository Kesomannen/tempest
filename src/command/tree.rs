use anyhow::bail;
use tracing::info;

use crate::{Context, Result, graph::DependencyGraph};

#[derive(Debug, clap::Parser)]
#[command(about = "Display the dependency tree for the current profile")]
pub struct TreeCommand {
    #[arg(
        short = 'p',
        long,
        value_name = "PACKAGE",
        help = "Show only the subtree rooted at the selected package(s)"
    )]
    packages: Vec<String>,

    #[arg(
        short = 'i',
        long,
        help = "Show packages that depend on the selected package(s)"
    )]
    reverse: bool,
}

impl super::Command for TreeCommand {
    async fn run(self, ctx: &Context) -> Result<()> {
        let profile = super::read_profile(ctx)?;

        if profile.lockfile.packages().is_empty() {
            info!("no mods installed, nothing to show");
            return Ok(());
        }

        let graph = DependencyGraph::new(&profile.lockfile);

        if self.reverse && self.packages.is_empty() {
            bail!("reverse tree requires at least one package target");
        }

        let rendered = if self.packages.is_empty() {
            graph.render()
        } else {
            graph.render_targeted(&self.packages, self.reverse)?
        };

        info!("\n{}", rendered);

        Ok(())
    }
}
