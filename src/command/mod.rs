mod add;
mod check;
mod clean;
mod config;
mod export;
mod fetch;
mod import;
mod install;
mod launch;
mod list;
mod new;
mod remove;
mod upgrade;

pub use add::AddCommand;
pub use check::CheckCommand;
pub use clean::CleanCommand;
pub use config::ConfigCommand;
pub use export::ExportCommand;
pub use fetch::FetchCommand;
pub use import::ImportCommand;
pub use install::InstallCommand;
pub use launch::LaunchCommand;
pub use list::ListCommand;
pub use new::NewCommand;
pub use remove::RemoveCommand;
pub use upgrade::UpgradeCommand;

use crate::{Context, Result};

pub(crate) fn read_profile(ctx: &Context) -> Result<crate::profile::Profile> {
    crate::profile::Profile::read_any_parent(&ctx.working_dir)
}

pub trait Command {
    async fn run(self, ctx: &Context) -> Result<()>;
}
