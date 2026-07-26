use std::time::Duration;

use anyhow::Context as _;
use loadsmith::PackageId;
use tracing::{debug, info, warn};
use tracing_indicatif::{span_ext::IndicatifSpanExt, style::ProgressStyle};

use crate::{Context, Result, Source};

#[derive(Debug, Default)]
pub struct Indexes(Vec<Index>);

#[derive(Debug)]
pub struct Index {
    source: Source,
    inner: loadsmith::thunderstore::SqliteIndex,
}

impl Indexes {
    pub fn new(indexes: Vec<Index>) -> Self {
        Self(indexes)
    }

    pub fn iter(&self) -> impl Iterator<Item = &Index> {
        self.0.iter()
    }

    pub fn get(&self, source: Source) -> Option<&Index> {
        self.0.iter().find(|index| index.source == source)
    }

    pub async fn prepare(&self, ctx: &Context, game: &str) -> Result {
        futures::future::try_join_all(self.iter().map(|index| async {
            index
                .prepare(ctx, game)
                .await
                .with_context(|| format!("failed to prepare {} index", index.source))
        }))
        .await?;
        Ok(())
    }

    pub async fn update(&self, game: &str) -> Result {
        futures::future::try_join_all(self.iter().map(|index| async {
            index
                .update(game)
                .await
                .with_context(|| format!("failed to update {} index", index.source))
        }))
        .await?;
        Ok(())
    }
}

impl Index {
    pub fn new(inner: loadsmith::thunderstore::SqliteIndex, source: Source) -> Self {
        Self { source, inner }
    }

    pub async fn prepare(&self, ctx: &Context, game: &str) -> Result {
        const MAX_AGE_BEFORE_WARN: Duration = Duration::from_hours(24 * 3); // 3 days

        let metadata = self.inner.community_metadata(game)?;
        let last_updated = metadata.and_then(|m| m.last_updated);

        if let Some(last_updated) = last_updated {
            let age = (chrono::Utc::now() - last_updated)
                .to_std()
                .unwrap_or(Duration::ZERO);

            let (should_auto_fetch, auto_fetch_enabled) = {
                let cfg = &ctx.config.borrow().auto_fetch;

                (cfg.enabled && age >= cfg.interval.0, cfg.enabled)
            };

            if should_auto_fetch {
                debug!(
                    %last_updated,
                    "mod index is older than auto-fetch interval"
                );
                self.update(game).await?;
            } else if age < MAX_AGE_BEFORE_WARN {
                debug!(
                    %last_updated,
                    "mod index is fresh"
                );
            } else if !auto_fetch_enabled {
                warn!(
                    %last_updated,
                    "mod index is older than 3 days, consider running `tempest fetch` to receive the latest mod updates"
                );

                info!(
                    "tip: you can enable auto-fetching of the mod index by running `tempest config set auto_fetch.enabled true`"
                );
            }
        } else {
            info!("mod index has not been built yet for {game}",);
            self.update(game).await?;
        }

        Ok(())
    }

    pub async fn update(&self, game: &str) -> Result {
        let span = tracing::info_span!("fetch");
        span.pb_set_style(&ProgressStyle::default_spinner());
        span.pb_set_message(&format!(
            "updating mod index for {game} from {}",
            self.source
        ));

        let _enter = span.enter();

        self.inner.update(game).await?;

        Ok(())
    }

    pub fn version_info(
        &self,
        id: &PackageId,
    ) -> Result<Option<Vec<loadsmith::registry::VersionInfo>>> {
        self.inner.version_info(id).map_err(Into::into)
    }

    pub fn search_packages(&self, query: &str, game: Option<&str>) -> Result<Vec<PackageId>> {
        self.inner.search_packages(query, game).map_err(Into::into)
    }
}
