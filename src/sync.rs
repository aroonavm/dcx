use std::io;
use std::path::{Path, PathBuf};

/// Represents a pair of paths to keep in sync.
#[derive(Debug, PartialEq, Clone)]
pub struct SyncPair {
    /// Host file path: ~/.claude.json
    pub source: PathBuf,
    /// Staging path: ~/.colima-mounts/.dcx-...-files/.claude.json
    pub staging: PathBuf,
}

/// Compute SHA256 hash of a file.
/// Returns None if the file is missing or unreadable.
pub fn sha256_file(path: &std::path::Path) -> Option<[u8; 32]> {
    use sha2::{Digest, Sha256};

    let content = std::fs::read(path).ok()?;
    let mut hasher = Sha256::new();
    hasher.update(&content);
    let result = hasher.finalize();
    let mut bytes = [0u8; 32];
    bytes.copy_from_slice(&result[..]);
    Some(bytes)
}

/// State tracking for a single sync pair
#[derive(Clone)]
struct SyncState {
    last_source_hash: Option<[u8; 32]>,
    last_staging_hash: Option<[u8; 32]>,
}

/// Run the sync daemon: watches parent directories of source and staging files via inotify (Linux) / FSEvents (macOS)
/// and keeps them in sync using SHA256 debounce.
///
/// This function runs indefinitely until SIGTERM is received.
pub fn run_sync_daemon(pairs: Vec<SyncPair>, pid_file: std::path::PathBuf) -> ! {
    use notify::Watcher;
    use std::collections::HashSet;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::mpsc;

    // Write PID to file
    let pid = std::process::id().to_string();
    let _ = std::fs::write(&pid_file, &pid);

    // Set up SIGTERM handler
    let term_flag = Arc::new(AtomicBool::new(false));
    let term_flag_clone = Arc::clone(&term_flag);
    let _ = signal_hook::flag::register(signal_hook::consts::SIGTERM, term_flag_clone);

    // Ignore SIGHUP so daemon survives when the terminal that ran `dcx up` closes.
    // Registering a handler (even one we never read) prevents the default kill action.
    let _ = signal_hook::flag::register(
        signal_hook::consts::SIGHUP,
        Arc::new(AtomicBool::new(false)),
    );

    // Initialize state tracking
    let mut states: Vec<SyncState> = pairs
        .iter()
        .map(|pair| SyncState {
            last_source_hash: sha256_file(&pair.source),
            last_staging_hash: sha256_file(&pair.staging),
        })
        .collect();

    // Build a set of watched filenames for efficient filtering (O(1) lookup)
    let watched_names: HashSet<std::ffi::OsString> = pairs
        .iter()
        .flat_map(|p| [p.source.file_name(), p.staging.file_name()])
        .flatten()
        .map(|n| n.to_owned())
        .collect();

    // Set up file watcher (inotify on Linux, FSEvents on macOS)
    let (tx, rx) = mpsc::channel();
    let mut watcher = match notify::recommended_watcher(tx) {
        Ok(w) => w,
        Err(e) => {
            eprintln!("Warning: Could not create file watcher: {e}");
            // Fall back to 1s polling if watcher fails
            loop_with_fallback(&pairs, &mut states, &term_flag, &pid_file);
        }
    };

    // Watch parent directories of source and staging files (handles atomic writes correctly)
    for pair in &pairs {
        if let Some(parent) = pair.source.parent() {
            let _ = watcher.watch(parent, notify::RecursiveMode::NonRecursive);
        }
        if let Some(parent) = pair.staging.parent() {
            let _ = watcher.watch(parent, notify::RecursiveMode::NonRecursive);
        }
    }

    // Main event loop: wait for file change notifications or SIGTERM
    loop {
        if term_flag.load(Ordering::Relaxed) {
            // Clean shutdown: remove PID file and exit
            let _ = std::fs::remove_file(&pid_file);
            std::process::exit(0);
        }

        // Wait for a file change event with 1s timeout to check SIGTERM regularly
        match rx.recv_timeout(std::time::Duration::from_secs(1)) {
            Ok(Ok(event)) => {
                // Only sync if the event involves one of our watched files
                let relevant = event.paths.iter().any(|p| {
                    p.file_name()
                        .map(|n| watched_names.contains(n))
                        .unwrap_or(false)
                });
                if relevant {
                    sync_all_pairs(&pairs, &mut states);
                }
            }
            Ok(Err(e)) => {
                eprintln!("Warning: file watcher error: {e}");
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {
                // Timeout: check SIGTERM and loop
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                // Watcher disconnected: fall back to polling
                loop_with_fallback(&pairs, &mut states, &term_flag, &pid_file);
            }
        }
    }
}

/// Returns whether a staging→source sync should proceed.
/// Rejects syncing an empty staging file over a non-empty source file
/// to prevent data loss (e.g., container writing a stripped config).
pub fn should_sync_to_source(source_len: u64, staging_len: u64) -> bool {
    !(staging_len == 0 && source_len > 0)
}

/// Atomically copy `src` to `dst` via temp file + rename.
/// The temp file is created in `dst`'s parent directory to guarantee
/// same-filesystem rename (atomic on POSIX).
fn atomic_copy(src: &Path, dst: &Path) -> io::Result<()> {
    let parent = dst.parent().unwrap_or(Path::new("."));
    let tmp = parent.join(format!(".dcx-sync-{}.tmp", std::process::id()));

    // Copy content to temp file
    match std::fs::copy(src, &tmp) {
        Ok(_) => {}
        Err(e) => {
            let _ = std::fs::remove_file(&tmp);
            return Err(e);
        }
    }

    // Atomic rename into place
    match std::fs::rename(&tmp, dst) {
        Ok(_) => Ok(()),
        Err(e) => {
            let _ = std::fs::remove_file(&tmp);
            Err(e)
        }
    }
}

/// Sync all pairs: check hashes and copy if needed
fn sync_all_pairs(pairs: &[SyncPair], states: &mut [SyncState]) {
    for (pair, state) in pairs.iter().zip(states.iter_mut()) {
        let src_hash = sha256_file(&pair.source);
        let stg_hash = sha256_file(&pair.staging);

        // Source changed → sync to staging (host is authority, no guard)
        if src_hash != state.last_source_hash && src_hash != stg_hash {
            if let Err(e) = atomic_copy(&pair.source, &pair.staging) {
                eprintln!(
                    "sync: {} -> {}: {e}",
                    pair.source.display(),
                    pair.staging.display()
                );
                continue;
            }
            state.last_source_hash = src_hash;
            state.last_staging_hash = src_hash;
        }
        // Staging changed → sync to source (apply size guard)
        else if stg_hash != state.last_staging_hash && stg_hash != src_hash {
            let src_len = std::fs::metadata(&pair.source)
                .map(|m| m.len())
                .unwrap_or(0);
            let stg_len = std::fs::metadata(&pair.staging)
                .map(|m| m.len())
                .unwrap_or(0);
            if !should_sync_to_source(src_len, stg_len) {
                eprintln!(
                    "sync: rejecting staging->source ({stg_len}B would overwrite {src_len}B)"
                );
                // Acknowledge change so we don't retry every cycle
                state.last_staging_hash = stg_hash;
                continue;
            }
            if let Err(e) = atomic_copy(&pair.staging, &pair.source) {
                eprintln!(
                    "sync: {} -> {}: {e}",
                    pair.staging.display(),
                    pair.source.display()
                );
                continue;
            }
            state.last_source_hash = stg_hash;
            state.last_staging_hash = stg_hash;
        }
    }
}

/// Fallback loop if watcher fails: poll every 1 second instead
fn loop_with_fallback(
    pairs: &[SyncPair],
    states: &mut [SyncState],
    term_flag: &std::sync::atomic::AtomicBool,
    pid_file: &std::path::Path,
) -> ! {
    use std::sync::atomic::Ordering;

    loop {
        if term_flag.load(Ordering::Relaxed) {
            let _ = std::fs::remove_file(pid_file);
            std::process::exit(0);
        }

        sync_all_pairs(pairs, states);
        std::thread::sleep(std::time::Duration::from_secs(1));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::NamedTempFile;

    #[test]
    fn sha256_file_known_content() {
        let mut file = NamedTempFile::new().unwrap();
        use std::io::Write;
        file.write_all(b"hello world").unwrap();
        file.flush().unwrap();

        let hash = sha256_file(file.path()).unwrap();

        // SHA256("hello world") = b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9
        let expected = [
            0xb9, 0x4d, 0x27, 0xb9, 0x93, 0x4d, 0x3e, 0x08, 0xa5, 0x2e, 0x52, 0xd7, 0xda, 0x7d,
            0xab, 0xfa, 0xc4, 0x84, 0xef, 0xe3, 0x7a, 0x53, 0x80, 0xee, 0x90, 0x88, 0xf7, 0xac,
            0xe2, 0xef, 0xcd, 0xe9,
        ];
        assert_eq!(hash, expected);
    }

    #[test]
    fn sha256_file_missing() {
        let missing_path = std::path::Path::new("/tmp/dcx-nonexistent-file-for-testing");
        // Ensure it doesn't exist
        let _ = fs::remove_file(missing_path);
        let hash = sha256_file(missing_path);
        assert_eq!(hash, None);
    }

    #[test]
    fn sha256_file_empty_file() {
        let file = NamedTempFile::new().unwrap();
        let hash = sha256_file(file.path()).unwrap();

        // SHA256("") = e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855
        let expected = [
            0xe3, 0xb0, 0xc4, 0x42, 0x98, 0xfc, 0x1c, 0x14, 0x9a, 0xfb, 0xf4, 0xc8, 0x99, 0x6f,
            0xb9, 0x24, 0x27, 0xae, 0x41, 0xe4, 0x64, 0x9b, 0x93, 0x4c, 0xa4, 0x95, 0x99, 0x1b,
            0x78, 0x52, 0xb8, 0x55,
        ];
        assert_eq!(hash, expected);
    }

    // --- Sync loop state tracking ---

    #[test]
    fn sync_loop_no_trigger_when_hashes_equal() {
        // Create two identical files
        let mut src = NamedTempFile::new().unwrap();
        use std::io::Write;
        src.write_all(b"same content").unwrap();
        src.flush().unwrap();

        let mut stg = NamedTempFile::new().unwrap();
        stg.write_all(b"same content").unwrap();
        stg.flush().unwrap();

        // Both hashes should be identical
        let src_hash = sha256_file(src.path()).unwrap();
        let stg_hash = sha256_file(stg.path()).unwrap();
        assert_eq!(src_hash, stg_hash);
    }

    #[test]
    fn sync_loop_source_change_detectable() {
        // Create source and staging with same initial content
        let mut src = NamedTempFile::new().unwrap();
        use std::io::Write;
        src.write_all(b"v1").unwrap();
        src.flush().unwrap();

        let mut stg = NamedTempFile::new().unwrap();
        stg.write_all(b"v1").unwrap();
        stg.flush().unwrap();

        let src_hash_1 = sha256_file(src.path()).unwrap();
        let stg_hash_1 = sha256_file(stg.path()).unwrap();
        assert_eq!(src_hash_1, stg_hash_1);

        // Change source
        std::fs::write(src.path(), b"v2").unwrap();
        let src_hash_2 = sha256_file(src.path()).unwrap();
        let stg_hash_2 = sha256_file(stg.path()).unwrap();

        // Source changed, staging didn't
        assert_ne!(src_hash_2, src_hash_1);
        assert_eq!(stg_hash_2, stg_hash_1);
        assert_ne!(src_hash_2, stg_hash_2);
    }

    #[test]
    fn sync_loop_staging_change_detectable() {
        // Create source and staging with same initial content
        let mut src = NamedTempFile::new().unwrap();
        use std::io::Write;
        src.write_all(b"v1").unwrap();
        src.flush().unwrap();

        let mut stg = NamedTempFile::new().unwrap();
        stg.write_all(b"v1").unwrap();
        stg.flush().unwrap();

        let src_hash_1 = sha256_file(src.path()).unwrap();
        let stg_hash_1 = sha256_file(stg.path()).unwrap();
        assert_eq!(src_hash_1, stg_hash_1);

        // Change staging
        std::fs::write(stg.path(), b"v2").unwrap();
        let src_hash_2 = sha256_file(src.path()).unwrap();
        let stg_hash_2 = sha256_file(stg.path()).unwrap();

        // Staging changed, source didn't
        assert_eq!(src_hash_2, src_hash_1);
        assert_ne!(stg_hash_2, stg_hash_1);
        assert_ne!(src_hash_2, stg_hash_2);
    }

    #[test]
    fn sync_copy_overwrites_destination() {
        // Test that fs::copy (used in sync) overwrites the destination file
        let mut src = NamedTempFile::new().unwrap();
        use std::io::Write;
        src.write_all(b"source content").unwrap();
        src.flush().unwrap();

        let mut dst = NamedTempFile::new().unwrap();
        dst.write_all(b"old destination").unwrap();
        dst.flush().unwrap();

        // fs::copy overwrites destination
        fs::copy(src.path(), dst.path()).expect("copy should succeed");
        let new_content = fs::read(dst.path()).expect("should be readable");
        assert_eq!(new_content, b"source content");
    }

    #[test]
    fn atomic_write_triggers_sync() {
        use std::io::Write;

        // Create source and staging files with initial content
        let src_dir = tempfile::TempDir::new().unwrap();
        let stg_dir = tempfile::TempDir::new().unwrap();

        let src_path = src_dir.path().join("test_file.json");
        let stg_path = stg_dir.path().join("test_file.json");

        // Write initial content to source and staging
        fs::write(&src_path, b"initial").unwrap();
        fs::write(&stg_path, b"initial").unwrap();

        // Create sync pair and state
        let pair = SyncPair {
            source: src_path.clone(),
            staging: stg_path.clone(),
        };

        let state = SyncState {
            last_source_hash: sha256_file(&src_path),
            last_staging_hash: sha256_file(&stg_path),
        };

        // Perform atomic write on source (write to temp, rename into place)
        let tmp_path = src_dir.path().join("test_file.json.tmp");
        {
            let mut tmp = fs::File::create(&tmp_path).unwrap();
            tmp.write_all(b"updated").unwrap();
        }
        fs::rename(&tmp_path, &src_path).unwrap();

        // Run sync logic
        sync_all_pairs(&[pair.clone()], &mut [state.clone()]);

        // Verify staging was updated with new content
        let stg_content = fs::read(&stg_path).unwrap();
        assert_eq!(stg_content, b"updated");
    }

    #[test]
    fn unrelated_file_in_same_dir_does_not_trigger_sync() {
        use std::io::Write;

        // Create source and staging files with initial content
        let src_dir = tempfile::TempDir::new().unwrap();
        let stg_dir = tempfile::TempDir::new().unwrap();

        let src_path = src_dir.path().join("watched_file.json");
        let stg_path = stg_dir.path().join("watched_file.json");

        // Write initial content to source and staging
        fs::write(&src_path, b"initial").unwrap();
        fs::write(&stg_path, b"initial").unwrap();

        // Create sync pair and state
        let pair = SyncPair {
            source: src_path.clone(),
            staging: stg_path.clone(),
        };

        let state = SyncState {
            last_source_hash: sha256_file(&src_path),
            last_staging_hash: sha256_file(&stg_path),
        };

        // Create an unrelated file in the same directory as source
        let unrelated_path = src_dir.path().join("unrelated_file.txt");
        {
            let mut f = fs::File::create(&unrelated_path).unwrap();
            f.write_all(b"unrelated content").unwrap();
        }

        // Run sync logic (should not trigger since unrelated_file.txt is not in watched_names)
        // We simulate the event filtering by directly calling sync_all_pairs
        // which will only sync if hashes have changed.
        // Since we didn't change the watched file, nothing should sync.
        sync_all_pairs(&[pair.clone()], &mut [state.clone()]);

        // Verify staging content is unchanged
        let stg_content = fs::read(&stg_path).unwrap();
        assert_eq!(stg_content, b"initial");

        // Clean up unrelated file
        fs::remove_file(&unrelated_path).unwrap();
    }

    // --- should_sync_to_source ---

    #[test]
    fn should_sync_both_empty() {
        assert!(should_sync_to_source(0, 0));
    }

    #[test]
    fn should_sync_rejects_empty_staging_over_nonempty_source() {
        assert!(!should_sync_to_source(100, 0));
    }

    #[test]
    fn should_sync_allows_nonempty_staging_over_empty_source() {
        assert!(should_sync_to_source(0, 50));
    }

    #[test]
    fn should_sync_allows_equal_sizes() {
        assert!(should_sync_to_source(100, 100));
    }

    #[test]
    fn should_sync_allows_smaller_staging() {
        assert!(should_sync_to_source(100, 50));
    }

    // --- atomic_copy ---

    #[test]
    fn atomic_copy_copies_content() {
        let dir = tempfile::TempDir::new().unwrap();
        let src = dir.path().join("src.txt");
        let dst = dir.path().join("dst.txt");
        fs::write(&src, b"hello atomic").unwrap();

        atomic_copy(&src, &dst).unwrap();
        assert_eq!(fs::read(&dst).unwrap(), b"hello atomic");
    }

    #[test]
    fn atomic_copy_overwrites_existing() {
        let dir = tempfile::TempDir::new().unwrap();
        let src = dir.path().join("src.txt");
        let dst = dir.path().join("dst.txt");
        fs::write(&src, b"new content").unwrap();
        fs::write(&dst, b"old content").unwrap();

        atomic_copy(&src, &dst).unwrap();
        assert_eq!(fs::read(&dst).unwrap(), b"new content");
    }

    #[test]
    fn atomic_copy_missing_source_returns_error() {
        let dir = tempfile::TempDir::new().unwrap();
        let src = dir.path().join("nonexistent.txt");
        let dst = dir.path().join("dst.txt");

        let result = atomic_copy(&src, &dst);
        assert!(result.is_err());
        // No temp file left behind
        assert!(!dst.exists());
    }

    #[test]
    fn atomic_copy_no_temp_left_on_success() {
        let dir = tempfile::TempDir::new().unwrap();
        let src = dir.path().join("src.txt");
        let dst = dir.path().join("dst.txt");
        fs::write(&src, b"content").unwrap();

        atomic_copy(&src, &dst).unwrap();

        // No .tmp files should remain
        let tmp_files: Vec<_> = fs::read_dir(dir.path())
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.path().extension().map_or(false, |ext| ext == "tmp"))
            .collect();
        assert!(tmp_files.is_empty());
    }

    // --- sync_all_pairs with size guard ---

    #[test]
    fn sync_rejects_empty_staging_to_source() {
        let dir = tempfile::TempDir::new().unwrap();
        let src = dir.path().join("source.json");
        let stg = dir.path().join("staging.json");

        // Source has real content, staging is empty
        fs::write(&src, b"important auth tokens here").unwrap();
        fs::write(&stg, b"important auth tokens here").unwrap();

        let pair = SyncPair {
            source: src.clone(),
            staging: stg.clone(),
        };
        let state = SyncState {
            last_source_hash: sha256_file(&src),
            last_staging_hash: sha256_file(&stg),
        };

        // Now empty the staging file (simulating container writing minimal config)
        fs::write(&stg, b"").unwrap();

        sync_all_pairs(&[pair], &mut [state]);

        // Source must NOT have been overwritten
        assert_eq!(fs::read(&src).unwrap(), b"important auth tokens here");
    }

    #[test]
    fn sync_all_pairs_simultaneous_source_and_staging_change_host_wins() {
        // Test case: both source and staging changed simultaneously
        // Expected: source is authority, so source overwrites staging
        let dir = tempfile::TempDir::new().unwrap();
        let src = dir.path().join("source.json");
        let stg = dir.path().join("staging.json");

        // Initial state: both files have same content
        fs::write(&src, b"v1").unwrap();
        fs::write(&stg, b"v1").unwrap();

        let initial_src_hash = sha256_file(&src).unwrap();
        let initial_stg_hash = sha256_file(&stg).unwrap();
        assert_eq!(initial_src_hash, initial_stg_hash);

        let pair = SyncPair {
            source: src.clone(),
            staging: stg.clone(),
        };
        let state = SyncState {
            last_source_hash: Some(initial_src_hash),
            last_staging_hash: Some(initial_stg_hash),
        };

        // BOTH files are modified independently
        fs::write(&src, b"host-change").unwrap();
        fs::write(&stg, b"container-change").unwrap();

        let src_hash_now = sha256_file(&src).unwrap();
        let stg_hash_now = sha256_file(&stg).unwrap();
        assert_ne!(src_hash_now, stg_hash_now);
        assert_ne!(src_hash_now, initial_src_hash);
        assert_ne!(stg_hash_now, initial_stg_hash);

        // Run sync: source changed AND source != staging → source wins
        sync_all_pairs(&[pair], &mut [state.clone()]);

        // Verify: staging was overwritten with source (host authority)
        assert_eq!(fs::read(&stg).unwrap(), b"host-change");
        // Source is unchanged
        assert_eq!(fs::read(&src).unwrap(), b"host-change");
    }
}
