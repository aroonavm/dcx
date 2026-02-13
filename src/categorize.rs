#![allow(dead_code)]

#[derive(Debug, PartialEq)]
pub enum MountStatus {
    /// Healthy bindfs mount with a running container.
    Active,
    /// Healthy bindfs mount but no running container.
    Orphaned,
    /// Mount entry exists in mount table but is inaccessible (FUSE process died, etc.).
    Stale,
    /// No bindfs mount found; just a leftover directory.
    Empty,
}

/// Categorize a dcx mount directory from observed state.
///
/// - `is_fuse_mounted`: the target appears in the mount table as a bindfs entry
/// - `is_accessible`: stat/ls of the mount point succeeds
/// - `has_container`: a running container is associated with this mount
pub fn categorize(is_fuse_mounted: bool, is_accessible: bool, has_container: bool) -> MountStatus {
    if !is_fuse_mounted {
        return MountStatus::Empty;
    }
    if !is_accessible {
        return MountStatus::Stale;
    }
    if has_container {
        MountStatus::Active
    } else {
        MountStatus::Orphaned
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn active_when_mounted_accessible_with_container() {
        assert_eq!(categorize(true, true, true), MountStatus::Active);
    }

    #[test]
    fn orphaned_when_mounted_accessible_no_container() {
        assert_eq!(categorize(true, true, false), MountStatus::Orphaned);
    }

    #[test]
    fn stale_when_mounted_but_inaccessible() {
        assert_eq!(categorize(true, false, false), MountStatus::Stale);
    }

    #[test]
    fn stale_when_mounted_inaccessible_even_with_container_flag() {
        assert_eq!(categorize(true, false, true), MountStatus::Stale);
    }

    #[test]
    fn empty_when_not_fuse_mounted() {
        assert_eq!(categorize(false, true, false), MountStatus::Empty);
    }

    #[test]
    fn empty_when_nothing_present() {
        assert_eq!(categorize(false, false, false), MountStatus::Empty);
    }
}
