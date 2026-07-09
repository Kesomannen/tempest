use loadsmith::PackageId;

use crate::{Context, Result};

#[derive(Debug, clap::Parser)]
#[command(about = "Remove a package from the current profile", alias = "rm")]
pub struct RemoveCommand {
    package: PackageId,
}

impl super::Command for RemoveCommand {
    async fn run(self, ctx: &Context) -> Result<()> {
        let mut profile = super::read_profile(ctx)?;

        let (package_id, _) = profile.manifest.mods.get_or_search(&self.package)?;
        profile.manifest.mods.remove(&package_id.clone());

        profile.write_manifest()?;

        profile.resolve_and_sync(ctx, false).await?;

        Ok(())
    }
}
