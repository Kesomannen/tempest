use tracing::info;

use crate::{Context, Result};

#[derive(Debug, clap::Parser)]
#[command(about = "List the packages in the current profile", alias = "ls")]
pub struct ListCommand;

impl super::Command for ListCommand {
    async fn run(self, ctx: &Context) -> Result<()> {
        let profile = super::read_profile(ctx)?;

        let mut builder = tabled::builder::Builder::new();

        builder.push_record(vec!["package", "version", "source", "transitive"]);

        let mut sorted_packages = profile.lockfile.packages().iter().collect::<Vec<_>>();
        sorted_packages.sort_by(|a, b| a.ref_.id().cmp(&b.ref_.id()));

        for package in sorted_packages {
            builder.push_record(vec![
                package.ref_.id().as_str(),
                &package.ref_.version().to_string(),
                package.source.as_str(),
                if package.transitive { "x" } else { "" },
            ]);
        }

        let mut table = builder.build();
        table.with(tabled::settings::Style::blank());

        info!("\n{table}");

        Ok(())
    }
}
