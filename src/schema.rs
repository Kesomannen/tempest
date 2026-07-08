use anyhow::Context as _;
use loadsmith::core::PackageRef;
use thunderstore::models::schema;

use crate::{Context, Result};

#[derive(Debug)]
pub struct ThunderstoreSchema {
    inner: schema::Schema,
}

impl ThunderstoreSchema {
    pub async fn fetch(ctx: &Context) -> Result<Self> {
        let schema = ctx
            .thunderstore
            .get_schema("dev")
            .await
            .context("failed to fetch thunderstore schema")?;

        Ok(Self { inner: schema })
    }

    pub fn game(&self, game: &str) -> Result<&schema::Game> {
        self.inner.games.get(game).context("unknown game")
    }

    pub fn make_loader(&self, game: &str) -> Result<Box<dyn loadsmith::loader::Loader>> {
        let config = self
            .game(game)?
            .r2modman
            .iter()
            .flatten()
            .next()
            .context("no r2modman config found for game")?;

        let loader = loadsmith::thunderstore::r2_config_to_loader(config)
            .context("failed to convert r2modman config to loader")?
            .with_context(|| format!("unsupported mod loader: {:?}", config.package_loader))?;

        Ok(loader)
    }

    pub fn make_platform(&self, game: &str) -> Result<loadsmith::platform::Platform> {
        let distribution = self
            .game(game)?
            .distributions
            .iter()
            .next()
            .context("no distribution found for game")?;

        loadsmith::thunderstore::distribution_into_platform(distribution.clone())?
            .context("unsupported distribution")
    }

    pub fn is_mod_loader(&self, pkg: &PackageRef) -> bool {
        self.inner
            .modloader_packages
            .iter()
            .any(|loader_pkg| loader_pkg.package_id.as_str() == pkg.id.as_str())
    }
}
