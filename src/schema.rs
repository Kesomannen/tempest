use anyhow::{Context as _, anyhow};
use loadsmith::PackageRef;
use thunderstore::models::schema;

use crate::{Context, Result};

#[derive(Debug)]
pub struct ThunderstoreSchema {
    inner: schema::Schema,
}

impl ThunderstoreSchema {
    pub async fn load(ctx: &Context) -> Result<Self> {
        let schema = ctx
            .thunderstore_client
            .get_schema("dev")
            .await
            .context("failed to fetch thunderstore schema")?;

        Ok(Self { inner: schema })
    }

    pub fn game(&self, game: &str) -> Result<&schema::Game> {
        self.inner
            .games
            .get(game)
            .ok_or_else(|| anyhow!("unknown game: {}", game))
    }

    pub fn make_loader(&self, game: &str) -> Result<Box<dyn loadsmith::Loader>> {
        let config = self
            .game(game)?
            .r2modman
            .iter()
            .flatten()
            .next()
            .context("no r2modman config found for game")?;

        let loader = loadsmith::thunderstore::r2_config_to_loader(config)
            .context("failed to convert r2modman config to loader")?;

        Ok(loader)
    }

    pub fn make_platforms(&self, game: &str) -> Result<Vec<loadsmith::Platform>> {
        self.game(game)?
            .distributions
            .iter()
            .cloned()
            .map(|dist| {
                loadsmith::thunderstore::distribution_into_platform(dist)
                    .context("failed to convert distribution to platform")
            })
            .collect()
    }

    pub fn make_platform(&self, game: &str) -> Result<loadsmith::Platform> {
        self.make_platforms(game)?
            .into_iter()
            .next()
            .context("no platforms found for game")
    }

    pub fn is_mod_loader(&self, pkg: &PackageRef) -> bool {
        self.inner.modloader_packages.iter().any(|loader_pkg| {
            loader_pkg
                .package_id
                .as_str()
                .eq_ignore_ascii_case(pkg.id().as_str())
        })
    }
}
