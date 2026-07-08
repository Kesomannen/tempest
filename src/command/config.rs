use crate::{Context, Result};

#[derive(Debug, clap::Parser)]
#[command(about = "")]
pub struct ConfigCommand {
    #[command(subcommand)]
    command: Subcommand,
}

#[derive(Debug, Clone, clap::Subcommand)]
enum Subcommand {
    #[command(about = "Set a config value")]
    Set { property: String, value: String },
}

impl super::Command for ConfigCommand {
    async fn run(self, ctx: &Context) -> Result<()> {
        match self.command {
            Subcommand::Set { property, value } => {
                let mut config = ctx.config.borrow_mut();

                config.set(&property, &value)?;
                config.write(ctx)?;
            }
        }

        Ok(())
    }
}
