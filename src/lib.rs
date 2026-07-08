use serde::{Deserialize, Serialize};

use crate::command::*;

mod command;
mod config;
mod context;
mod fmt;
mod index;
mod manifest;
mod profile;
mod schema;
mod store;
mod util;

pub use config::Config;
pub use context::Context;

// type Error = anyhow::Error;
type Result<T = ()> = anyhow::Result<T>;

#[derive(Debug, clap::Parser)]
#[command(version, about, long_about = None)]
pub struct Cli {
    #[arg(short, long, global = true, help = "Enable verbose logging")]
    pub verbose: bool,

    #[arg(
        short,
        long,
        global = true,
        help = "Prevent any modifications to the lockfile"
    )]
    pub locked: bool,

    #[command(subcommand)]
    command: Subcommand,
}

#[derive(Debug, clap::Subcommand)]
enum Subcommand {
    New(NewCommand),
    Install(InstallCommand),
    Upgrade(UpgradeCommand),
    Fetch(FetchCommand),
    Check(CheckCommand),
    Launch(LaunchCommand),
    Remove(RemoveCommand),
    Import(ImportCommand),
    Export(ExportCommand),
    Add(AddCommand),
    List(ListCommand),
    Clean(CleanCommand),
    Config(ConfigCommand),
}

impl Command for Subcommand {
    async fn run(self, ctx: &Context) -> anyhow::Result<()> {
        match self {
            Subcommand::New(command) => command.run(ctx).await,
            Subcommand::Install(command) => command.run(ctx).await,
            Subcommand::Upgrade(command) => command.run(ctx).await,
            Subcommand::Fetch(command) => command.run(ctx).await,
            Subcommand::Check(command) => command.run(ctx).await,
            Subcommand::Launch(command) => command.run(ctx).await,
            Subcommand::Remove(command) => command.run(ctx).await,
            Subcommand::Import(command) => command.run(ctx).await,
            Subcommand::Export(command) => command.run(ctx).await,
            Subcommand::Add(command) => command.run(ctx).await,
            Subcommand::List(command) => command.run(ctx).await,
            Subcommand::Clean(command) => command.run(ctx).await,
            Subcommand::Config(command) => command.run(ctx).await,
        }
    }
}

impl Cli {
    pub async fn run(self, ctx: &Context) -> Result {
        self.command.run(ctx).await
    }
}

#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(default)]
struct ExtraImportData {
    community: Option<String>,
}
