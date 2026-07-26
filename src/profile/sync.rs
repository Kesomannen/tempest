use std::io::Cursor;

use anyhow::{Context as _, bail};
use futures::{StreamExt, TryStreamExt, stream};
use loadsmith::{Checksum, InstallRuleset, Loader, LockedPackage, PackageRef};
use tokio::io::AsyncReadExt;
use tracing::{debug, info, trace};
use tracing_indicatif::{span_ext::IndicatifSpanExt, style::ProgressStyle};

use crate::{Context, Result, profile::Profile, schema::ThunderstoreSchema};

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
            debug!("uninstalling {}", package.ref_().id());

            profile.state.uninstall(&package.ref_().id())?;

            profile.write_state()?;
        }
    }

    if !to_add.is_empty() {
        donwload_and_install(to_add, ctx, profile).await?;
    }

    Ok(())
}

async fn donwload_and_install(
    mut packages: Vec<LockedPackage>,
    ctx: &Context,
    profile: &mut Profile,
) -> Result {
    packages.sort_by(|a, b| a.size.cmp(&b.size).then(a.ref_.cmp(&b.ref_)).reverse());

    download_uncached_packages(&packages, ctx).await?;

    let res = install_packages(packages, ctx, profile).await;

    profile.write_state()?;

    res
}

async fn install_packages(
    packages: Vec<LockedPackage>,
    ctx: &Context,
    profile: &mut Profile,
) -> Result {
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
const EXTRACT_CONCURRENCY: usize = 4;

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
            let bytes = download(&package, ctx)
                .await
                .with_context(|| format!("error while downloading {}", package.ref_.id()))?;

            if let Some(expected_checksum) = &package.checksum {
                trace!(
                    package = %package.ref_.id(),
                    %expected_checksum,
                    "verifying checksum"
                );

                let checksum = Checksum::compute(&bytes[..], expected_checksum.algorithm())
                    .context("failed to compute checksum")?;

                if &checksum != expected_checksum {
                    bail!(
                        "checksum mismatch for {}: expected {expected_checksum}, got {checksum}",
                        package.ref_.id()
                    );
                }
            }

            Ok::<_, anyhow::Error>((package, bytes))
        })
        .buffer_unordered(DOWNLOAD_CONCURRENCY)
        .map(|result| async move {
            let (package, bytes) = result?;

            // need 'static lifetime for spawn_blocking
            let store = ctx.store.clone();

            tokio::task::spawn_blocking(move || {
                store
                    .add(&package.store_entry(), Cursor::new(bytes))
                    .with_context(|| format!("error while extracting {}", package.ref_.id()))?;

                Ok::<_, anyhow::Error>(package)
            })
            .await
            .context("extraction task panicked")?
        })
        .buffer_unordered(EXTRACT_CONCURRENCY)
        .try_for_each(|package| {
            debug!("finished {}", package.ref_.id());
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

    package_span.pb_set_message(package.ref_.id().as_str());
    if let Some(size) = package.size {
        package_span.pb_set_length(size);
    }

    let _package_enter = package_span.enter();

    debug!("downloading {}", package.ref_.id());

    let mut vec = Vec::with_capacity(package.size.unwrap_or(0) as usize);
    if let Some(path) = package.url.strip_prefix("file://") {
        let mut file = tokio::fs::File::open(path).await?;
        file.read_to_end(&mut vec).await?;

        package_span.pb_inc(vec.len() as u64);
    } else {
        let mut stream = ctx.http.get(&package.url).send().await?.bytes_stream();

        while let Some(chunk) = stream.try_next().await? {
            vec.extend_from_slice(&chunk);
            if package.size.is_some() {
                package_span.pb_inc(chunk.len() as u64);
            }
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
