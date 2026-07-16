use std::{cell::RefCell, path::PathBuf};

use loadsmith::{PackageStore, RegistrySet, thunderstore::SqliteIndex};

use crate::{Config, Result, profile::Profile};

#[derive(Debug)]
pub struct Context {
    pub(crate) http: reqwest::Client,
    pub(crate) thunderstore: thunderstore::Client,
    pub(crate) registry_set: RegistrySet,
    pub(crate) index: SqliteIndex,
    pub(crate) working_dir: PathBuf,
    pub(crate) home_dir: PathBuf,
    pub(crate) locked: bool,
    pub(crate) config: RefCell<Config>,
    pub(crate) store: PackageStore,
}

impl Context {
    #[must_use]
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        http: reqwest::Client,
        thunderstore: thunderstore::Client,
        registry_set: RegistrySet,
        index: SqliteIndex,
        working_dir: PathBuf,
        home_dir: PathBuf,
        locked: bool,
        config: Config,
        store: PackageStore,
    ) -> Self {
        Self {
            http,
            thunderstore,
            registry_set,
            index,
            working_dir,
            home_dir,
            locked,
            config: RefCell::new(config),
            store,
        }
    }

    pub(crate) fn read_profile(&self) -> Result<Profile> {
        Profile::read_any_parent(&self.working_dir)
    }
}
