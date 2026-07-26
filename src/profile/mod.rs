use std::path::{Path, PathBuf};

use anyhow::{Context as _, bail};
use camino::Utf8Path;
use loadsmith::{Lockfile, ProfileState, ProfileStateData};
use tracing::debug;
use tracing_indicatif::{span_ext::IndicatifSpanExt, style::ProgressStyle};

use crate::{Context, Result, manifest::Manifest, source::Source, util};

mod sync;

#[derive(Debug)]
pub struct Profile {
    pub manifest: Manifest,
    pub lockfile: Lockfile,
    pub state: ProfileState,
}

impl Profile {
    pub fn new(manifest: Manifest, lockfile: Lockfile, state: ProfileState) -> Self {
        Self {
            manifest,
            lockfile,
            state,
        }
    }

    pub fn create(path: impl Into<PathBuf>, manifest: Manifest) -> Result<Self> {
        const GIT_IGNORE: &str = r"# tempest profile state
/_state
";

        let path = path.into();
        let lockfile = Lockfile::default();
        let state = ProfileState::new(path, ProfileStateData::default());

        let this = Self {
            manifest,
            lockfile,
            state,
        };

        this.write_all()?;

        std::fs::write(this.path().join(".gitignore"), GIT_IGNORE)
            .context("failed to write .gitignore")?;

        Ok(this)
    }

    const MANIFEST_FILE_NAME: &str = "tempest.toml";
    const LOCKFILE_FILE_NAME: &str = "tempest.lock";
    const PROFILE_STATE_FILE_NAME: &str = "_state/tempest.json";

    pub fn read_any_parent(path: impl Into<PathBuf>) -> Result<Self> {
        let mut path = path.into();

        loop {
            if Self::is_profile_dir(&path) {
                debug!("found profile manifest at `{}`", path.display());
                return Self::read(&path);
            }

            if !path.pop() {
                bail!("profile manifest not found in any parent directory");
            }
        }
    }

    pub fn is_profile_dir(path: impl AsRef<Path>) -> bool {
        path.as_ref().join(Self::MANIFEST_FILE_NAME).exists()
    }

    pub fn read(path: impl Into<PathBuf>) -> Result<Profile> {
        let path = path.into();

        let manifest = Self::read_manifest(&path).context("failed to read manifest")?;
        let lockfile = Self::read_lockfile(&path).context("failed to read lockfile")?;
        let state = Self::read_profile_state(path).context("failed to read profile state")?;

        Ok(Self::new(manifest, lockfile, state))
    }

    fn read_manifest(path: &Path) -> Result<Manifest> {
        util::read_toml(path.join(Self::MANIFEST_FILE_NAME))
    }

    fn read_lockfile(path: &Path) -> Result<Lockfile> {
        let path = path.join(Self::LOCKFILE_FILE_NAME);
        if path.exists() {
            util::read_json(&path)
        } else {
            Ok(Lockfile::default())
        }
    }

    fn read_profile_state(path: PathBuf) -> Result<ProfileState> {
        let state_path = path.join(Self::PROFILE_STATE_FILE_NAME);
        let data = if state_path.exists() {
            util::read_json(&state_path)?
        } else {
            ProfileStateData::default()
        };

        Ok(ProfileState::new(path, data))
    }

    pub fn write_all(&self) -> Result {
        self.write_manifest()?;
        self.write_lockfile()?;
        self.write_state()?;
        Ok(())
    }

    pub fn write_manifest(&self) -> Result {
        util::write_toml(self.path().join(Self::MANIFEST_FILE_NAME), &self.manifest)
            .context("failed to write manifest")
    }

    pub fn write_lockfile(&self) -> Result {
        util::write_json(self.path().join(Self::LOCKFILE_FILE_NAME), &self.lockfile)
            .context("failed to write lockfile")
    }

    pub fn write_state(&self) -> Result {
        util::write_json(
            self.path().join(Self::PROFILE_STATE_FILE_NAME),
            self.state.data(),
        )
        .context("failed to write profile state")
    }

    pub fn path(&self) -> &Path {
        self.state.path()
    }

    pub fn path_utf8(&self) -> &Utf8Path {
        Utf8Path::from_path(self.path()).expect("profile path should be valid UTF-8")
    }

    pub fn name(&self) -> &str {
        self.path_utf8()
            .file_name()
            .expect("profile directory should have file name")
    }

    pub fn game(&self) -> &str {
        &self.manifest.profile.game
    }

    pub async fn resolve_and_sync(&mut self, ctx: &Context, update: bool) -> Result {
        self.resolve_and_update_lockfile(ctx, update).await?;
        self.sync(ctx).await?;

        Ok(())
    }

    async fn resolve_and_update_lockfile(&mut self, ctx: &Context, update: bool) -> Result {
        let new_lockfile = self.resolve(ctx, update).await?;

        let diff = self.lockfile.diff(&new_lockfile);
        if ctx.locked && !diff.is_empty() {
            bail!("profile is locked, cannot update lockfile");
        }

        crate::fmt::log_lockfile_diff(&diff);

        if !diff.is_empty() {
            self.lockfile = new_lockfile;

            self.write_lockfile()?;
        }

        Ok(())
    }

    pub fn default_source(&self) -> Source {
        const DEFAULT_DEFAULT_SOURCE: Source = Source::Thunderstore;

        self.manifest
            .profile
            .default_source
            .unwrap_or(DEFAULT_DEFAULT_SOURCE)
    }

    async fn resolve(&self, ctx: &Context, update: bool) -> Result<Lockfile> {
        let span = tracing::info_span!("resolve_manifest");
        span.pb_set_style(&ProgressStyle::default_spinner());
        span.pb_set_message("resolving dependencies...");

        let _enter = span.enter();

        let dependencies = self
            .manifest
            .mods
            .clone()
            .into_dependencies(self.default_source());

        loadsmith::resolve(
            dependencies,
            &ctx.registry_set,
            if update { None } else { Some(&self.lockfile) },
        )
        .await
        .context("error while resolving manifest")
    }

    pub async fn sync(&mut self, ctx: &Context) -> Result {
        sync::sync_profile(ctx, self).await
    }
}
