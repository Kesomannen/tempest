use std::{fmt::Display, time::Duration};

use anyhow::Context as _;
use futures::{StreamExt, TryStreamExt, stream};
use loadsmith::{InstallRuleset, Loader, LockedPackage, PackageRef};
use tracing::{debug, info, warn};
use tracing_indicatif::{span_ext::IndicatifSpanExt, style::ProgressStyle};

use crate::{Context, Result, profile::Profile, schema::ThunderstoreSchema};

pub async fn sync_profile(ctx: &Context, profile: &mut Profile) -> Result<bool> {
    let diff = profile.state.diff_lockfile(&profile.lockfile);

    if diff.is_empty() {
        info!("profile is up to date");
        return Ok(false);
    }

    let to_remove = diff.to_remove().cloned().collect::<Vec<_>>();
    let to_add = diff.to_add().cloned().collect::<Vec<_>>();

    for package in to_remove {
        debug!("uninstalling {}", package.ref_().id());

        profile
            .state
            .uninstall(&package.ref_().id())
            .with_context(|| format!("error uninstalling {}", package.ref_()))?;

        profile.write_state()?;
    }

    if !to_add.is_empty() {
        install_missing_packages(to_add, ctx, profile).await?;
    }

    Ok(true)
}

async fn install_missing_packages(
    mut packages: Vec<LockedPackage>,
    ctx: &Context,
    profile: &mut Profile,
) -> Result {
    packages.sort_by(|a, b| a.size.cmp(&b.size).then(a.ref_.cmp(&b.ref_)).reverse());

    download_uncached_packages(&packages, ctx).await?;

    let res = do_install(packages, ctx, profile).await;

    profile.write_state()?;

    res
}

async fn do_install(packages: Vec<LockedPackage>, ctx: &Context, profile: &mut Profile) -> Result {
    let span = tracing::info_span!("install_packages");
    span.pb_set_style(&ProgressStyle::default_spinner());
    span.pb_set_message("installing mods...");

    let _enter = span.enter();
    let schema = ThunderstoreSchema::load(ctx).await?;
    let loader = schema.make_loader(profile.game())?;

    for package in packages {
        debug!("installing {}", package.ref_.id());

        let ruleset = ruleset_for_package(&schema, &*loader, &package.ref_);
        profile
            .state
            .install_from_store(package.into_store_entry(), ruleset, &ctx.store)?;

        profile.write_state()?;

        span.pb_inc(1);
    }

    Ok(())
}

const DOWNLOAD_CONCURRENCY: usize = 4;

async fn download_uncached_packages(packages: &[LockedPackage], ctx: &Context) -> Result {
    let to_download: Vec<LockedPackage> = packages
        .iter()
        .filter(|locked| !ctx.store.contains(&locked.store_entry()))
        .cloned()
        .collect();

    let span = tracing::info_span!("download_packages");
    span.pb_set_style(
        &ProgressStyle::default_bar()
            .template("[{elapsed_precise}] [{bar:40.cyan/blue}] {pos:>10}/{len:10}: {msg}")
            .unwrap()
            .progress_chars("=>-"),
    );
    span.pb_set_message("downloading packages...");
    span.pb_set_length(to_download.len() as u64);

    let _enter = span.enter();

    stream::iter(to_download)
        .map(|package| async move {
            let store_entry = package.store_entry();
            let target_path = ctx.store.reserve(&store_entry)?;

            loadsmith::download_and_extract(package.url.clone(), target_path, |url| async {
                #[derive(Debug)]
                struct DownloadError(anyhow::Error);

                impl Display for DownloadError {
                    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(f, "{}", self.0)
                    }
                }

                impl std::error::Error for DownloadError {}

                // force the async block to take ownership of url
                let url = url;
                download_package(&url, &package, ctx)
                    .await
                    .map_err(DownloadError)
            })
            .await
            .with_context(|| format!("error while extracting {}", package.ref_.id()))?;

            Ok::<_, anyhow::Error>(package)
        })
        .buffer_unordered(DOWNLOAD_CONCURRENCY)
        .try_for_each(|package| {
            debug!("finished {}", package.ref_.id());
            span.pb_inc(1);

            async { Ok(()) }
        })
        .await?;

    Ok(())
}

async fn download_package(
    url: &reqwest::Url,
    package: &LockedPackage,
    ctx: &Context,
) -> Result<Vec<u8>> {
    let span = tracing::info_span!(parent: None, "download_package");
    span.pb_set_style(
        &ProgressStyle::default_bar()
            .template(
                "[{elapsed_precise}] [{bar:40.cyan/blue}] {bytes:>10}/{total_bytes:10}: {msg}",
            )
            .unwrap()
            .progress_chars("=>-"),
    );

    span.pb_set_message(package.ref_.id().as_str());
    if let Some(size) = package.size {
        span.pb_set_length(size);
    }

    let _package_enter = span.enter();

    debug!("downloading {}", package.ref_.id());

    let mut vec = Vec::with_capacity(package.size.unwrap_or(0) as usize);
    download_with_retries(url, package, ctx, &mut vec, &span).await?;

    Ok(vec)
}

async fn download_with_retries(
    url: &reqwest::Url,
    package: &LockedPackage,
    ctx: &Context,
    buf: &mut Vec<u8>,
    span: &tracing::Span,
) -> Result {
    let mut retries = 0;
    loop {
        match try_download(url, package, buf, ctx, span).await {
            Ok(()) => break Ok(()),
            Err(err) if retries >= 3 => {
                debug!(retries, "download failed after retries, giving up");
                return Err(err);
            }
            Err(err) => {
                let backoff = Duration::from_secs(2u64.pow(retries).min(30));

                warn!(
                    error = %err,
                    ?backoff,
                    retry = retries,
                    "download failed, retrying",
                );

                span.pb_set_position(0);
                buf.clear();

                retries += 1;

                tokio::time::sleep(backoff).await;
            }
        }
    }
}

async fn try_download(
    url: &reqwest::Url,
    package: &LockedPackage,
    buf: &mut Vec<u8>,
    ctx: &Context,
    span: &tracing::Span,
) -> Result {
    let mut stream = ctx
        .http
        .get(url.clone())
        .send()
        .await?
        .error_for_status()?
        .bytes_stream();

    while let Some(chunk) = stream.try_next().await? {
        buf.extend_from_slice(&chunk);
        if package.size.is_some() {
            span.pb_inc(chunk.len() as u64);
        }
    }

    Ok(())
}

fn ruleset_for_package<'a>(
    schema: &ThunderstoreSchema,
    loader: &'a dyn Loader,
    pkg: &PackageRef,
) -> InstallRuleset<'a> {
    if schema.is_mod_loader(pkg) {
        loader.loader_install_rules()
    } else {
        loader.package_install_rules()
    }
}
