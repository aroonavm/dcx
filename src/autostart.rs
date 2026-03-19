#![allow(dead_code)]

use std::path::{Path, PathBuf};

use crate::cli::AutostartAction;
use crate::cmd;
use crate::exit_codes;
use crate::progress;

// ── Pure functions ────────────────────────────────────────────────────────────

/// Return the path to the autostart service file for the current platform.
///
/// Linux: `~/.config/systemd/user/colima.service`
/// macOS: `~/Library/LaunchAgents/io.colima.autostart.plist`
pub fn service_file_path(home: &Path) -> PathBuf {
    #[cfg(target_os = "linux")]
    {
        home.join(".config/systemd/user/colima.service")
    }
    #[cfg(target_os = "macos")]
    {
        home.join("Library/LaunchAgents/io.colima.autostart.plist")
    }
}

/// Generate the content of the autostart service file.
///
/// Linux: systemd user service unit with ExecStart/ExecStop.
/// macOS: launchd plist with ProgramArguments and RunAtLoad.
pub fn generate_service_content(colima_bin: &str) -> String {
    #[cfg(target_os = "linux")]
    {
        format!(
            "[Unit]\nDescription=Colima container runtime\nAfter=network-online.target\nWants=network-online.target\n\n[Service]\nType=oneshot\nRemainAfterExit=yes\nExecStart={} start\nExecStop={} stop\n\n[Install]\nWantedBy=default.target\n",
            colima_bin, colima_bin
        )
    }
    #[cfg(target_os = "macos")]
    {
        format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>io.colima.autostart</string>
    <key>ProgramArguments</key>
    <array>
        <string>{}</string>
        <string>start</string>
    </array>
    <key>RunAtLoad</key>
    <true/>
    <key>StandardOutPath</key>
    <string>/tmp/colima.log</string>
    <key>StandardErrorPath</key>
    <string>/tmp/colima.err</string>
</dict>
</plist>
"#,
            colima_bin
        )
    }
}

/// Check whether autostart is currently configured.
///
/// Returns true if the service file exists on the filesystem.
pub fn is_configured(home: &Path) -> bool {
    service_file_path(home).exists()
}

// ── Entry point ───────────────────────────────────────────────────────────────

/// Run `dcx autostart`.
///
/// Returns the exit code that `main` should pass to `std::process::exit`.
pub fn run_autostart(home: &Path, action: &AutostartAction) -> i32 {
    match action {
        AutostartAction::Enable => run_enable(home),
        AutostartAction::Disable => run_disable(home),
        AutostartAction::Status => run_status(home),
    }
}

fn run_enable(home: &Path) -> i32 {
    // 1. Find colima binary.
    let colima_bin = match cmd::run_capture("which", &["colima"]) {
        Ok(out) if out.status == 0 => out.stdout.trim().to_string(),
        _ => {
            eprintln!("colima not found in PATH. Is Colima installed?");
            return exit_codes::PREREQ_NOT_FOUND;
        }
    };

    // 2. Compute service file path.
    let service_path = service_file_path(home);
    progress::step(&format!(
        "Configuring autostart at {}",
        service_path.display()
    ));

    // 3. Create parent directory if needed.
    if let Some(parent) = service_path.parent()
        && !parent.exists()
        && let Err(e) = std::fs::create_dir_all(parent)
    {
        eprintln!(
            "Failed to create parent directory {}: {}",
            parent.display(),
            e
        );
        return exit_codes::RUNTIME_ERROR;
    }

    // 4. Write service content.
    let content = generate_service_content(&colima_bin);
    if let Err(e) = std::fs::write(&service_path, &content) {
        eprintln!(
            "Failed to write service file {}: {}",
            service_path.display(),
            e
        );
        return exit_codes::RUNTIME_ERROR;
    }

    // 5. Activate service (platform-specific).
    #[cfg(target_os = "linux")]
    {
        progress::step("Reloading systemd configuration...");
        if let Err(e) = cmd::run_capture("systemctl", &["--user", "daemon-reload"]) {
            eprintln!("Failed to reload systemd: {}", e);
            return exit_codes::RUNTIME_ERROR;
        }

        progress::step("Enabling Colima autostart...");
        if let Err(e) = cmd::run_capture("systemctl", &["--user", "enable", "colima"]) {
            eprintln!("Failed to enable colima service: {}", e);
            return exit_codes::RUNTIME_ERROR;
        }

        progress::step("Starting Colima...");
        match cmd::run_capture("systemctl", &["--user", "start", "colima"]) {
            Ok(out) if out.status == 0 => {
                println!("✓ Colima autostart enabled");
            }
            Ok(_) => {
                eprintln!("Failed to start colima (may already be running)");
            }
            Err(e) => {
                eprintln!("Error starting colima: {}", e);
                return exit_codes::RUNTIME_ERROR;
            }
        }
    }

    #[cfg(target_os = "macos")]
    {
        progress::step("Loading LaunchAgent...");
        match cmd::run_capture(
            "launchctl",
            &["load", service_path.to_string_lossy().as_ref()],
        ) {
            Ok(out) if out.status == 0 => {
                println!("✓ Colima autostart enabled");
            }
            Ok(_) => {
                eprintln!("Note: launchctl reported a non-zero status (may already be loaded)");
                println!("✓ Service file created at {}", service_path.display());
            }
            Err(e) => {
                eprintln!("Failed to load LaunchAgent: {}", e);
                return exit_codes::RUNTIME_ERROR;
            }
        }
    }

    exit_codes::SUCCESS
}

fn run_disable(home: &Path) -> i32 {
    let service_path = service_file_path(home);

    // If not configured, print message and exit success.
    if !is_configured(home) {
        println!("Autostart is not configured");
        return exit_codes::SUCCESS;
    }

    progress::step("Disabling Colima autostart...");

    // Deactivate service (platform-specific).
    #[cfg(target_os = "linux")]
    {
        if let Err(e) = cmd::run_capture("systemctl", &["--user", "stop", "colima"]) {
            eprintln!("Warning: failed to stop colima service: {}", e);
        }

        if let Err(e) = cmd::run_capture("systemctl", &["--user", "disable", "colima"]) {
            eprintln!("Warning: failed to disable colima service: {}", e);
        }
    }

    #[cfg(target_os = "macos")]
    {
        if let Err(e) = cmd::run_capture(
            "launchctl",
            &["unload", service_path.to_string_lossy().as_ref()],
        ) {
            eprintln!("Warning: failed to unload LaunchAgent: {}", e);
        }
    }

    // Delete the service file.
    if let Err(e) = std::fs::remove_file(&service_path) {
        eprintln!(
            "Failed to remove service file {}: {}",
            service_path.display(),
            e
        );
        return exit_codes::RUNTIME_ERROR;
    }

    println!("✓ Colima autostart disabled");
    exit_codes::SUCCESS
}

fn run_status(home: &Path) -> i32 {
    let service_path = service_file_path(home);
    let configured = is_configured(home);

    println!("Service file: {}", service_path.display());
    println!("Configured: {}", if configured { "yes" } else { "no" });

    if !configured {
        return exit_codes::SUCCESS;
    }

    // Check live service state (platform-specific).
    #[cfg(target_os = "linux")]
    {
        let is_enabled = cmd::run_capture("systemctl", &["--user", "is-enabled", "colima"])
            .map(|out| out.status == 0)
            .unwrap_or(false);

        let is_active = cmd::run_capture("systemctl", &["--user", "is-active", "colima"])
            .map(|out| out.status == 0)
            .unwrap_or(false);

        println!("Enabled: {}", if is_enabled { "yes" } else { "no" });
        println!("Active: {}", if is_active { "yes" } else { "no" });
    }

    #[cfg(target_os = "macos")]
    {
        let is_loaded = cmd::run_capture("launchctl", &["list", "io.colima.autostart"])
            .map(|out| out.status == 0)
            .unwrap_or(false);

        println!("Loaded: {}", if is_loaded { "yes" } else { "no" });
    }

    exit_codes::SUCCESS
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- service_file_path ---

    #[test]
    fn service_file_path_returns_valid_pathbuf() {
        let home = Path::new("/home/user");
        let path = service_file_path(home);
        assert!(!path.as_os_str().is_empty());
    }

    #[test]
    #[cfg(target_os = "linux")]
    fn service_file_path_linux_contains_systemd_user() {
        let home = Path::new("/home/user");
        let path = service_file_path(home);
        assert!(
            path.to_string_lossy()
                .contains(".config/systemd/user/colima.service")
        );
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn service_file_path_macos_contains_launchagents() {
        let home = Path::new("/Users/user");
        let path = service_file_path(home);
        assert!(
            path.to_string_lossy()
                .contains("Library/LaunchAgents/io.colima.autostart.plist")
        );
    }

    // --- generate_service_content ---

    #[test]
    fn generate_service_content_includes_binary_path() {
        let content = generate_service_content("/usr/local/bin/colima");
        assert!(content.contains("/usr/local/bin/colima"));
    }

    #[test]
    #[cfg(target_os = "linux")]
    fn generate_service_content_linux_is_systemd_format() {
        let content = generate_service_content("/usr/local/bin/colima");
        assert!(content.contains("[Unit]"));
        assert!(content.contains("[Service]"));
        assert!(content.contains("Type=oneshot"));
        assert!(content.contains("RemainAfterExit=yes"));
        assert!(content.contains("ExecStart="));
        assert!(content.contains("ExecStop="));
        assert!(content.contains("[Install]"));
        assert!(content.contains("WantedBy=default.target"));
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn generate_service_content_macos_is_plist_format() {
        let content = generate_service_content("/usr/local/bin/colima");
        assert!(content.contains("<?xml"));
        assert!(content.contains("<plist"));
        assert!(content.contains("io.colima.autostart"));
        assert!(content.contains("ProgramArguments"));
        assert!(content.contains("RunAtLoad"));
    }

    // --- is_configured ---

    #[test]
    fn is_configured_returns_false_for_nonexistent_home() {
        let home = Path::new("/nonexistent/home/path");
        assert!(!is_configured(home));
    }
}
