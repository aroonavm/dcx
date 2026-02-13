#![allow(dead_code)]

//! Mount categorization for `dcx` relay-directory entries.
//!
//! # Spec note — Stale vs Empty
//!
//! The spec (architecture.md § "dcx clean") defines four categories where "Stale mount"
//! covers both "bindfs mount doesn't exist" and "bindfs mount is unhealthy". This module
//! refines that into two distinct variants:
//!
//! | Spec category  | This module         | Criterion                                   |
//! |----------------|---------------------|---------------------------------------------|
//! | Stale mount    | `Stale`             | In mount table, but inaccessible            |
//! | Stale mount    | `Empty`             | Not in mount table (no FUSE entry at all)   |
//! | Empty directory| `Empty`             | Not in mount table (leftover dir)           |
//! | Active mount   | `Active`            | In mount table, accessible, has container   |
//! | Orphaned mount | `Orphaned`          | In mount table, accessible, no container    |
//!
//! The split is necessary because `dcx clean` should skip the unmount step when there is
//! no mount table entry (`Empty`), whereas it should attempt unmount for `Stale`. Without
//! a state file (which the spec explicitly rejects), "was previously mounted" and "never
//! mounted" are indistinguishable, so both map to `Empty`.

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
///
/// # Design note — Stale vs Empty
///
/// The spec prose defines "Stale mount" as "bindfs mount doesn't exist or is unhealthy"
/// (architecture.md §"dcx clean"). In practice, "doesn't exist" (no mount table entry)
/// is indistinguishable from "was never mounted" without a state file — which the spec
/// explicitly rejects. We therefore split the spec's "Stale" into two cases:
///
/// - `is_fuse_mounted && !is_accessible` → `Stale`: entry is in the mount table but
///   inaccessible (FUSE process died). Caller should attempt `fusermount -u` before removal.
/// - `!is_fuse_mounted` → `Empty`: no mount table entry; just a leftover directory.
///   Caller can remove it directly without an unmount attempt.
///
/// Both cases are cleaned by `dcx clean`; the only behavioral difference is whether a
/// (likely-to-fail) unmount is attempted first.
///
/// Note: inputs where `!is_fuse_mounted && has_container` are logically impossible in
/// practice but are still handled deterministically (→ `Empty`).
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

    #[test]
    fn empty_when_not_mounted_inaccessible_with_container_flag() {
        // Logically impossible in practice but must be deterministic.
        assert_eq!(categorize(false, false, true), MountStatus::Empty);
    }

    #[test]
    fn empty_when_not_mounted_accessible_with_container_flag() {
        // Logically impossible in practice but must be deterministic.
        assert_eq!(categorize(false, true, true), MountStatus::Empty);
    }
}
