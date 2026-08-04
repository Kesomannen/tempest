use std::{cell::RefCell, path::PathBuf};

use loadsmith::{PackageStore, RegistrySet};

use crate::{Config, Result, index::Indexes, profile::Profile};

#[derive(Debug)]
pub struct Context {
    pub(crate) http: reqwest::Client,
    pub(crate) working_dir: PathBuf,
    pub(crate) home_dir: PathBuf,
    pub(crate) locked: bool,
    pub(crate) config: RefCell<Config>,
    pub(crate) store: PackageStore,
    pub(crate) registry_set: RegistrySet,
    pub(crate) thunderstore_client: thunderstore::Client,
    // pub(crate) hexium_client: thunderstore::Client,
    pub(crate) indexes: Indexes,
}

impl Context {
    #[must_use]
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        http: reqwest::Client,
        working_dir: PathBuf,
        home_dir: PathBuf,
        locked: bool,
        config: Config,
        store: PackageStore,
        registry_set: RegistrySet,
        thunderstore_client: thunderstore::Client,
        // hexium_client: thunderstore::Client,
        indexes: Indexes,
    ) -> Self {
        Self {
            http,
            working_dir,
            home_dir,
            locked,
            config: RefCell::new(config),
            store,
            registry_set,
            thunderstore_client,
            // hexium_client,
            indexes,
        }
    }

    pub(crate) fn read_profile(&self) -> Result<Profile> {
        Profile::read_any_parent(&self.working_dir)
    }
}
