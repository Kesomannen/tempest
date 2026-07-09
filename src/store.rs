use std::path::{Path, PathBuf};

use loadsmith::PackageRef;
use walkdir::WalkDir;

use crate::Context;

pub fn contains(ctx: &Context, package: &PackageRef) -> bool {
    package_dir(ctx, package).exists()
}

pub fn package_dir(ctx: &Context, package: &PackageRef) -> PathBuf {
    let prefix = package
        .id
        .as_str()
        .chars()
        .take(2)
        .collect::<String>()
        .to_lowercase();

    let store_dir = root(ctx)
        .join(prefix)
        .join(package.id.as_str())
        .join(package.version.to_string());

    store_dir
}

pub fn entries(ctx: &Context) -> impl Iterator<Item = PathBuf> {
    WalkDir::new(root(ctx))
        .min_depth(3)
        .max_depth(3)
        .into_iter()
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.file_type().is_dir())
        .map(|entry| entry.into_path())
}

pub fn unused_entries(ctx: &Context) -> impl Iterator<Item = PathBuf> {
    entries(ctx).filter(|entry| is_unused(entry))
}

fn is_unused(path: &Path) -> bool {
    WalkDir::new(path)
        .into_iter()
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.file_type().is_file())
        .all(|file| {
            let Some(meta) = file.metadata().ok() else {
                return false;
            };

            #[cfg(unix)]
            {
                use std::os::unix::fs::MetadataExt;

                meta.nlink() <= 1
            }

            #[cfg(not(unix))]
            {
                false
            }
        })
}

fn root(ctx: &Context) -> PathBuf {
    ctx.home_dir.join("store")
}
