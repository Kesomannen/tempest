use std::{
    fs::{self, File},
    io::{Cursor, Read},
    sync::Arc,
};

use anyhow::Context as _;
use futures::{StreamExt, TryStreamExt, stream};
use loadsmith::{InstallRuleset, Loader, LockedPackage, PackageRef};
use tracing::{debug, info};
use tracing_indicatif::{span_ext::IndicatifSpanExt, style::ProgressStyle};

use crate::{Context, Result, profile::Profile, schema::ThunderstoreSchema, store};

pub async fn sync_profile(ctx: &Context, profile: &mut Profile) -> Result {
    let diff = profile.state.diff_lockfile(&profile.lockfile);

    if diff.is_empty() {
        info!("profile is up to date");
        return Ok(());
    }

    let to_remove = diff.to_remove().cloned().collect::<Vec<_>>();
    let to_add = diff.to_add().cloned().collect::<Vec<_>>();

    if !to_remove.is_empty() {
        for package in to_remove {
            debug!("uninstalling {}", package.ref_.id);

            profile.state.uninstall(&package.ref_.id)?;

            profile.write_state()?;
        }
    }

    if !to_add.is_empty() {
        install_packages(to_add, ctx, profile).await?;
    }

    Ok(())
}

async fn install_packages(
    mut packages: Vec<LockedPackage>,
    ctx: &Context,
    profile: &mut Profile,
) -> Result {
    packages.sort_by_key(|pkg| pkg.size.unwrap_or(0));
    packages.reverse();

    let schema = ThunderstoreSchema::fetch(ctx).await.map(Arc::new)?;
    let loader: Arc<dyn Loader> = schema.make_loader(profile.game()).map(Arc::from)?;

    download_uncached_packages(schema.clone(), loader.clone(), &packages, &ctx).await?;

    let span = tracing::info_span!("install_packages");
    span.pb_set_style(&ProgressStyle::default_spinner());
    span.pb_set_message("installing packages...");

    let _enter = span.enter();

    for package in packages {
        let ruleset = ruleset_for_package(&schema, &*loader, &package.ref_);
        let store_dir = store::package_dir(ctx, &package.ref_);
        profile
            .state
            .install(package.ref_, ruleset, &store_dir, false)
            .context("failed to install package")?;

        profile.write_state()?;

        span.pb_inc(1);
    }

    Ok(())
}

/// Number of packages downloaded over HTTP concurrently.
/// Bounded by network/bandwidth, not CPU — can be relatively high.
const DOWNLOAD_CONCURRENCY: usize = 4;

/// Number of packages being extracted (blocking, CPU + disk I/O) concurrently.
/// Bounded by CPU cores, kept separate so a slow extract doesn't stall downloads.
const EXTRACT_CONCURRENCY: usize = 4;

async fn download_uncached_packages(
    schema: Arc<ThunderstoreSchema>,
    loader: Arc<dyn Loader>,
    packages: &[LockedPackage],
    ctx: &&Context,
) -> Result {
    let to_download: Vec<LockedPackage> = packages
        .iter()
        .filter(|pkg| !store::contains(ctx, &pkg.ref_))
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
        .map(|package| {
            // Stage 1: async download only. No blocking CPU work here.
            async move {
                let bytes = download(&package, ctx)
                    .await
                    .with_context(|| format!("error while downloading {}", package.ref_.id))?;

                Ok::<_, anyhow::Error>((package, bytes))
            }
        })
        .buffer_unordered(DOWNLOAD_CONCURRENCY)
        .map(|result| {
            let schema = schema.clone();
            let loader = loader.clone();

            // Stage 2: extraction, moved to a blocking thread pool so it
            // doesn't hog the async runtime while other downloads are in flight.
            async move {
                let (package, bytes) = result?;

                let target = store::package_dir(ctx, &package.ref_);

                tokio::task::spawn_blocking(move || {
                    let ruleset = ruleset_for_package(&schema, &*loader, &package.ref_);

                    fs::create_dir_all(&target)
                        .context("failed to create package store directory")?;

                    loadsmith::extract(Cursor::new(bytes), &package.ref_, ruleset, &target)
                        .with_context(|| format!("error while extracting {}", package.ref_.id))?;

                    Ok::<_, anyhow::Error>(package)
                })
                .await
                .context("extraction task panicked")?
            }
        })
        .buffer_unordered(EXTRACT_CONCURRENCY)
        .try_for_each(|package| {
            debug!("finished {}", package.ref_.id);
            span.pb_inc(1);

            async { Ok(()) }
        })
        .await?;

    Ok(())
}

async fn download(package: &LockedPackage, ctx: &Context) -> Result<Vec<u8>> {
    let package_span = tracing::info_span!(parent: None, "download_package");
    package_span.pb_set_style(
        &ProgressStyle::default_bar()
            .template(
                "[{elapsed_precise}] [{bar:40.cyan/blue}] {bytes:>10}/{total_bytes:10}: {msg}",
            )
            .unwrap()
            .progress_chars("=>-"),
    );

    package_span.pb_set_message(package.ref_.id.as_str());
    if let Some(size) = package.size {
        package_span.pb_set_length(size);
    }

    let _package_enter = package_span.enter();

    debug!("downloading {}", package.ref_.id);

    let mut vec = Vec::with_capacity(package.size.unwrap_or(0) as usize);
    if let Some(path) = package.url.strip_prefix("file://") {
        let mut file = File::open(path)?;
        file.read_to_end(&mut vec)?;

        package_span.pb_inc(vec.len() as u64);
    } else {
        let mut stream = ctx.http.get(&package.url).send().await?.bytes_stream();

        while let Some(chunk) = stream.try_next().await? {
            vec.extend_from_slice(&chunk);
            package_span.pb_inc(chunk.len() as u64);
        }
    }

    Ok(vec)
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
