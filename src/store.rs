use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

use anyhow::bail;
use loadsmith::PackageRef;
use tracing::{trace, warn};
use walkdir::WalkDir;

use crate::Result;

#[derive(Debug, Clone)]
pub struct PackageStore {
    root: Arc<Path>,
}

impl PackageStore {
    pub fn new(root: impl Into<Arc<Path>>) -> Self {
        Self { root: root.into() }
    }

    pub(crate) fn remove(&self, path: impl AsRef<Path>) -> Result {
        let path = path.as_ref();

        if !path.strip_prefix(&self.root).is_ok() {
            bail!(
                "cannot remove package store directory `{}`: not in store root `{}`",
                path.display(),
                self.root.display()
            );
        }

        std::fs::remove_dir_all(path)?;
        remove_empty_parents(path)?;

        Ok(())
    }

    pub(crate) fn contains(&self, package: &PackageRef) -> bool {
        self.path_of(package).exists()
    }

    pub(crate) fn path_of(&self, package: &PackageRef) -> PathBuf {
        let prefix = package
            .id()
            .as_str()
            .chars()
            .take(2)
            .collect::<String>()
            .to_lowercase();

        self.root
            .join(prefix)
            .join(package.id().as_str())
            .join(package.version().to_string())
    }

    pub(crate) fn entries(&self) -> impl Iterator<Item = PathBuf> {
        WalkDir::new(&self.root)
            .min_depth(3)
            .max_depth(3)
            .into_iter()
            .filter_map(|entry| entry.ok())
            .filter(|entry| entry.file_type().is_dir())
            .map(walkdir::DirEntry::into_path)
    }

    pub(crate) fn unused_entries(&self) -> impl Iterator<Item = PathBuf> {
        self.entries().filter(|entry| Self::is_unused(entry))
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
}

fn remove_empty_parents(path: impl Into<PathBuf>) -> std::io::Result<()> {
    use std::io::ErrorKind;

    let mut path = path.into();

    while path.pop() {
        match std::fs::remove_dir(&path) {
            Ok(_) => {
                trace!(path = %path.display(), "removed empty directory");
            }
            Err(err)
                if matches!(
                    err.kind(),
                    ErrorKind::DirectoryNotEmpty | ErrorKind::NotFound
                ) =>
            {
                break;
            }
            Err(err) if err.kind() == ErrorKind::PermissionDenied => {
                warn!(path = %path.display(), "permission denied while removing empty directories");
                break;
            }
            Err(err) => return Err(err.into()),
        }
    }

    Ok(())
}
