use std::{
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context as _, bail};
use camino::Utf8Path;
use loadsmith::{LockedPackage, Lockfile, ProfileState, ProfileStateData, manifest::Diff};
use tracing::{debug, info};
use tracing_indicatif::{span_ext::IndicatifSpanExt, style::ProgressStyle};

use crate::{
    Context, Result, manifest::Manifest, schema::ThunderstoreSchema, source::Source, util,
};

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
        let path = path.into();
        let lockfile = Lockfile::default();
        let state = ProfileState::new(path, ProfileStateData::default());

        let this = Self {
            manifest,
            lockfile,
            state,
        };

        this.write_all()?;

        Ok(this)
    }

    pub fn write_git_ignore_from_schema(&self, schema: &ThunderstoreSchema) -> Result {
        match schema.make_loader(self.game()) {
            Ok(loader) => self.write_git_ignore(&loader.package_config_dirs()),
            Err(err) => {
                debug!(
                    %err,
                    "failed to create loader for game '{}', writing default .gitignore",
                    self.game()
                );
                self.write_git_ignore::<PathBuf>(&[])
            }
        }
    }

    pub fn write_git_ignore<P: AsRef<Path>>(&self, config_dirs: &[P]) -> Result {
        let mut text = String::from(
            r"# exclude everything by default
/**

# include gitignore, manifest and lockfile
!/.gitignore
!/tempest.toml
!/tempest.lock
",
        );

        if !config_dirs.is_empty() {
            let config_dirs_string = config_dirs
                .iter()
                .map(|p| unignore_directory(p))
                .collect::<String>();

            text.push_str(&format!(
                "\n# include config directories\n{}\n",
                config_dirs_string
            ));
        }

        fs::write(self.path().join(".gitignore"), text).context("failed to write .gitignore")?;

        Ok(())
    }

    const MANIFEST_FILE_NAME: &str = "tempest.toml";
    const LOCKFILE_FILE_NAME: &str = "tempest.lock";
    const PROFILE_STATE_FILE_NAME: &str = ".tempest/state.json";

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
            util::read_toml(&path)
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
        util::write_toml(self.path().join(Self::LOCKFILE_FILE_NAME), &self.lockfile)
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

    pub fn default_source(&self) -> Source {
        self.manifest
            .profile
            .default_source
            .unwrap_or(Source::Thunderstore)
    }

    pub async fn resolve_and_sync(&mut self, ctx: &Context, update: bool) -> Result<bool> {
        self.resolve_and_update_lockfile(ctx, update).await?;
        let made_changes = self.sync(ctx).await?;

        if !update {
            self.check_for_updates(ctx).await?;
        }

        Ok(made_changes)
    }

    async fn resolve_and_update_lockfile(&mut self, ctx: &Context, update: bool) -> Result {
        let new_lockfile = self.resolve(ctx, update).await?;

        let diff = self.lockfile.diff(&new_lockfile);
        if ctx.locked && !diff.is_empty() {
            bail!("profile is locked, cannot update lockfile");
        }

        Self::log_lockfile_diff(&diff);

        if !diff.is_empty() {
            self.lockfile = new_lockfile;

            self.write_lockfile()?;
        }

        Ok(())
    }

    fn log_lockfile_diff(diff: &Diff<LockedPackage, LockedPackage>) {
        if diff.is_empty() {
            debug!("lockfile satisfies manifest, no changes needed");
            return;
        }

        for package in &diff.added {
            info!("added {}", package.ref_);
        }

        for package in &diff.removed {
            info!("removed {}", package.ref_.id());
        }

        for (old, new) in &diff.changed {
            if old.ref_.version() == new.ref_.version() {
                info!("{} changed", old.ref_.id());
                continue;
            }

            info!(
                "{} {}: {} -> {}",
                if old.ref_.version() < new.ref_.version() {
                    "upgraded"
                } else {
                    "downgraded"
                },
                old.ref_.id(),
                old.ref_.version(),
                new.ref_.version()
            )
        }
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

    pub async fn check_for_updates(&self, ctx: &Context) -> Result {
        let updated_lockfile = self.resolve(ctx, true).await?;
        let diff = self.lockfile.diff(&updated_lockfile);

        if diff.is_empty() {
            debug!("no updates found");
            return Ok(());
        }

        Self::log_updates(&diff);

        Ok(())
    }

    fn log_updates(diff: &Diff<LockedPackage, LockedPackage>) {
        if diff.changed.is_empty() {
            return;
        }

        info!("there are {} mod updates available:", diff.changed.len());

        for (old, new) in &diff.changed {
            if old.ref_.version() == new.ref_.version() {
                info!("{} changed", old.ref_.id());
                continue;
            }

            info!(
                "   {}: {} -> {}",
                old.ref_.id(),
                old.ref_.version(),
                new.ref_.version()
            )
        }

        info!("run `tempest upgrade` to upgrade");
    }

    pub async fn sync(&mut self, ctx: &Context) -> Result<bool> {
        sync::sync_profile(ctx, self).await
    }
}

fn unignore_directory(path: impl AsRef<Path>) -> String {
    let mut buf = PathBuf::new();
    let mut out = String::new();

    for comp in path.as_ref().components() {
        buf.push(comp);

        let path_str = buf.to_string_lossy().replace('\\', "/");
        let line = format!("!/{path_str}/");
        out.push_str(&line);
        out.push('\n');
    }

    let path_str = path.as_ref().to_string_lossy().replace('\\', "/");
    let line = format!("!/{path_str}/**");
    out.push_str(&line);

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gitignore_unignore_directory() {
        let path = Path::new("BepInEx/config");
        let result = unignore_directory(path);
        assert_eq!(
            result,
            r"!/BepInEx/
    !/BepInEx/config/
    !/BepInEx/config/**"
        );
    }
}
