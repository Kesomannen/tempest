use loadsmith::{LockedPackage, manifest::Diff};
use tracing::{debug, info};

pub fn log_lockfile_diff(diff: &Diff<LockedPackage, LockedPackage>) {
    if diff.is_empty() {
        debug!("lockfile satisfies manifest, no changes needed");
        return;
    }

    for package in &diff.added {
        info!("added {}", package.ref_);
    }

    for package in &diff.removed {
        info!("removed {}", package.ref_.id);
    }

    for (old, new) in &diff.changed {
        if old.ref_.version < new.ref_.version {
            info!(
                "upgraded {}: {} -> {}",
                old.ref_.id, old.ref_.version, new.ref_.version
            );
        } else {
            info!(
                "downgraded {}: {} -> {}",
                old.ref_.id, old.ref_.version, new.ref_.version
            );
        }
    }
}
