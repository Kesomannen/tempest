use std::time::Duration;

use tracing::{debug, info, warn};
use tracing_indicatif::{span_ext::IndicatifSpanExt, style::ProgressStyle};

use crate::{Context, Result};

pub async fn check(ctx: &Context, game: &str) -> Result {
    const MAX_AGE_BEFORE_WARN: Duration = Duration::from_hours(24 * 3); // 3 days

    let metadata = ctx.index.community_metadata(game)?;
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
                "package index is older than auto-fetch interval"
            );
            update(ctx, game).await?;
        } else if age < MAX_AGE_BEFORE_WARN {
            debug!(
                %last_updated,
                "package index is fresh"
            );
        } else if !auto_fetch_enabled {
            warn!(
                %last_updated,
                "package index is older than 3 days, consider running `tempest fetch` to receive the latest mod updates"
            );

            info!(
                "tip: you can enable auto-fetching of the package index by running `tempest config set auto_fetch.enabled true`"
            );
        }
    } else {
        info!("package index has not been built yet for {game}",);
        update(ctx, game).await?;
    }

    Ok(())
}

pub async fn update(ctx: &Context, game: &str) -> Result {
    let span = tracing::info_span!("fetch");
    span.pb_set_style(&ProgressStyle::default_spinner());
    span.pb_set_message("updating package index...");

    let _enter = span.enter();

    ctx.index.update(game).await?;

    Ok(())
}
