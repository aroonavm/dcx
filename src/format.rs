#![allow(dead_code)]

/// A row in the `dcx status` table.
pub struct StatusRow {
    /// Original workspace path, or None if it cannot be resolved.
    pub workspace: Option<String>,
    /// Mount point name (e.g. `dcx-myproject-a1b2c3d4`).
    pub mount: String,
    /// Docker container short ID, or None if no container.
    pub container: Option<String>,
    /// Human-readable state string (e.g. `running`, `stale mount`).
    pub state: String,
}

/// Format the `dcx status` output table.
///
/// Returns `"No active workspaces."` when `rows` is empty.
pub fn format_status_table(rows: &[StatusRow]) -> String {
    if rows.is_empty() {
        return "No active workspaces.".to_string();
    }
    let header = format!(
        "{:<30} {:<30} {:<12} {}",
        "WORKSPACE", "MOUNT", "CONTAINER", "STATE"
    );
    let mut lines = vec![header];
    for row in rows {
        let workspace = row.workspace.as_deref().unwrap_or("(unknown)");
        let container = row.container.as_deref().unwrap_or("(none)");
        lines.push(format!(
            "{:<30} {:<30} {:<12} {}",
            workspace, row.mount, container, row.state
        ));
    }
    lines.join("\n")
}

/// A single prerequisite check result for `dcx doctor`.
pub struct DoctorCheck {
    /// Short description of the check (e.g. `bindfs installed`).
    pub name: String,
    /// Whether the check passed.
    pub passed: bool,
    /// On pass: optional version string. On fail: optional fix hint.
    pub detail: Option<String>,
}

/// Format the full `dcx doctor` report.
pub fn format_doctor_report(checks: &[DoctorCheck]) -> String {
    let mut lines = vec!["Checking prerequisites...".to_string()];
    let all_passed = !checks.is_empty() && checks.iter().all(|c| c.passed);

    for check in checks {
        if check.passed {
            let detail = check
                .detail
                .as_deref()
                .map(|d| format!(" ({})", d))
                .unwrap_or_default();
            lines.push(format!("  \u{2713} {}{}", check.name, detail));
        } else {
            lines.push(format!("  \u{2717} {}", check.name));
            if let Some(fix) = &check.detail {
                lines.push(format!("    Fix: {}", fix));
            }
        }
    }

    if all_passed {
        lines.push(String::new());
        lines.push("All checks passed.".to_string());
    }
    lines.join("\n")
}

/// An entry in the `dcx clean` summary.
pub struct CleanEntry {
    /// Original workspace path, or None if not recoverable.
    pub workspace: Option<String>,
    /// Mount point name (e.g. `dcx-myproject-a1b2c3d4`).
    pub mount: String,
    /// State before cleaning (e.g. `orphaned`, `stale`, `empty dir`).
    pub was: String,
    /// Action taken (e.g. `unmounted`, `removed`).
    pub action: String,
}

/// A plan for cleaning a mount (used by dry-run preview).
#[derive(Clone, Debug)]
pub struct DryRunPlan {
    /// Mount name (e.g. dcx-myproject-a1b2c3d4)
    pub mount_name: String,
    /// State before cleaning: "running", "orphaned", "stale", or "empty dir"
    pub state: String,
    /// Container ID if present
    pub container_id: Option<String>,
    /// Runtime image ID if present
    pub runtime_image_id: Option<String>,
    /// Whether a dcx-base:<mount_name> tag exists (purge=true)
    pub has_base_image_tag: bool,
    /// Volumes if purge=true
    pub volumes: Vec<String>,
    /// Whether mounted
    pub is_mounted: bool,
}

/// Format the `dcx clean --dry-run` preview.
///
/// Shows what would be cleaned without executing any changes.
pub fn format_dry_run(plans: &[DryRunPlan]) -> String {
    if plans.is_empty() {
        return "Nothing to clean.".to_string();
    }
    let mut lines = vec!["Would clean:".to_string()];
    for plan in plans {
        lines.push(format!("  {}  ({})", plan.mount_name, plan.state));
        if let Some(container_id) = &plan.container_id {
            lines.push(format!("    - Stop and remove container {}", container_id));
        }
        if let Some(image_id) = &plan.runtime_image_id {
            lines.push(format!("    - Remove runtime image {}", image_id));
        }
        if plan.has_base_image_tag {
            lines.push(format!(
                "    - Remove base image tag dcx-base:{}  [purge]",
                plan.mount_name
            ));
        }
        for volume in &plan.volumes {
            lines.push(format!("    - Remove volume {}  [purge]", volume));
        }
        if plan.is_mounted {
            lines.push("    - Unmount bindfs".to_string());
        }
        lines.push("    - Remove mount directory".to_string());
    }
    lines.join("\n")
}

/// Format the `dcx clean` summary.
pub fn format_clean_summary(entries: &[CleanEntry], active_left: usize) -> String {
    let header = if active_left > 0 {
        format!(
            "Cleaned {} mounts ({} active mounts left untouched):",
            entries.len(),
            active_left
        )
    } else {
        format!("Cleaned {} mounts:", entries.len())
    };

    let mut lines = vec![header];
    for entry in entries {
        let left = match &entry.workspace {
            Some(ws) => format!("{}  \u{2192}  {}", ws, entry.mount),
            None => entry.mount.clone(),
        };
        lines.push(format!(
            "  {:<52} was: {:<12} \u{2192} {}",
            left, entry.was, entry.action
        ));
    }
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- format_status_table ---

    #[test]
    fn status_table_empty_rows() {
        assert_eq!(format_status_table(&[]), "No active workspaces.");
    }

    #[test]
    fn status_table_header_present() {
        let rows = vec![StatusRow {
            workspace: Some("/home/user/project-a".to_string()),
            mount: "dcx-project-a-a1b2c3d4".to_string(),
            container: Some("abc123".to_string()),
            state: "running".to_string(),
        }];
        let out = format_status_table(&rows);
        assert!(out.contains("WORKSPACE"), "missing WORKSPACE header");
        assert!(out.contains("MOUNT"), "missing MOUNT header");
        assert!(out.contains("CONTAINER"), "missing CONTAINER header");
        assert!(out.contains("STATE"), "missing STATE header");
    }

    #[test]
    fn status_table_row_data_present() {
        let rows = vec![StatusRow {
            workspace: Some("/home/user/project-a".to_string()),
            mount: "dcx-project-a-a1b2c3d4".to_string(),
            container: Some("abc123".to_string()),
            state: "running".to_string(),
        }];
        let out = format_status_table(&rows);
        assert!(out.contains("/home/user/project-a"));
        assert!(out.contains("dcx-project-a-a1b2c3d4"));
        assert!(out.contains("abc123"));
        assert!(out.contains("running"));
    }

    #[test]
    fn status_table_columns_are_aligned() {
        let rows = vec![StatusRow {
            workspace: Some("/home/user/project-a".to_string()),
            mount: "dcx-project-a-a1b2c3d4".to_string(),
            container: Some("abc123".to_string()),
            state: "running".to_string(),
        }];
        let out = format_status_table(&rows);
        let mut lines = out.lines();
        let header = lines.next().unwrap();
        let data = lines.next().unwrap();
        let mount_col = header.find("MOUNT").unwrap();
        let mount_data_col = data.find("dcx-").unwrap();
        assert_eq!(
            mount_col, mount_data_col,
            "MOUNT column misaligned:\nheader: {header}\ndata:   {data}"
        );
    }

    #[test]
    fn status_table_unknown_workspace_shown() {
        let rows = vec![StatusRow {
            workspace: None,
            mount: "dcx-project-c-i9j0k1l2".to_string(),
            container: None,
            state: "stale mount".to_string(),
        }];
        let out = format_status_table(&rows);
        assert!(out.contains("(unknown)"));
        assert!(out.contains("(none)"));
        assert!(out.contains("stale mount"));
    }

    // --- format_doctor_report ---

    #[test]
    fn doctor_report_starts_with_checking_prerequisites() {
        let out = format_doctor_report(&[]);
        assert!(out.starts_with("Checking prerequisites..."));
        assert!(!out.contains("All checks passed."));
    }

    #[test]
    fn doctor_report_all_passed_message() {
        let checks = vec![DoctorCheck {
            name: "bindfs installed".to_string(),
            passed: true,
            detail: Some("v1.17.2".to_string()),
        }];
        let out = format_doctor_report(&checks);
        assert!(out.contains("All checks passed."), "got: {out}");
        assert!(out.contains("✓ bindfs installed (v1.17.2)"), "got: {out}");
    }

    #[test]
    fn doctor_report_passed_check_without_detail() {
        // A passed check with detail: None must not render empty parentheses.
        let checks = vec![DoctorCheck {
            name: "bindfs installed".to_string(),
            passed: true,
            detail: None,
        }];
        let out = format_doctor_report(&checks);
        assert!(out.contains("✓ bindfs installed"), "got: {out}");
        assert!(
            !out.contains("()"),
            "must not render empty parens, got: {out}"
        );
    }

    #[test]
    fn doctor_report_failed_check_shows_cross_and_fix() {
        let checks = vec![DoctorCheck {
            name: "bindfs not installed".to_string(),
            passed: false,
            detail: Some("sudo apt install bindfs".to_string()),
        }];
        let out = format_doctor_report(&checks);
        assert!(!out.contains("All checks passed."), "got: {out}");
        assert!(out.contains("✗ bindfs not installed"), "got: {out}");
        assert!(out.contains("Fix: sudo apt install bindfs"), "got: {out}");
    }

    #[test]
    fn doctor_report_mixed_checks() {
        let checks = vec![
            DoctorCheck {
                name: "bindfs installed".to_string(),
                passed: true,
                detail: None,
            },
            DoctorCheck {
                name: "devcontainer not installed".to_string(),
                passed: false,
                detail: Some("npm install -g @devcontainers/cli".to_string()),
            },
        ];
        let out = format_doctor_report(&checks);
        assert!(!out.contains("All checks passed."));
        assert!(out.contains("✓ bindfs installed"));
        assert!(out.contains("✗ devcontainer not installed"));
    }

    // --- format_clean_summary ---

    #[test]
    fn clean_summary_no_active_left() {
        let entries = vec![CleanEntry {
            workspace: Some("/home/user/project-b".to_string()),
            mount: "dcx-project-b-e5f6g7h8".to_string(),
            was: "orphaned".to_string(),
            action: "unmounted".to_string(),
        }];
        let out = format_clean_summary(&entries, 0);
        assert!(out.starts_with("Cleaned 1 mounts:"), "got: {out}");
        assert!(out.contains("/home/user/project-b"));
        assert!(out.contains("dcx-project-b-e5f6g7h8"));
        assert!(out.contains("was: orphaned"));
        assert!(out.contains("unmounted"));
    }

    #[test]
    fn clean_summary_with_active_left() {
        let entries = vec![CleanEntry {
            workspace: None,
            mount: "dcx-project-c-i9j0k1l2".to_string(),
            was: "stale".to_string(),
            action: "unmounted".to_string(),
        }];
        let out = format_clean_summary(&entries, 2);
        assert!(
            out.starts_with("Cleaned 1 mounts (2 active mounts left untouched):"),
            "got: {out}"
        );
        assert!(out.contains("dcx-project-c-i9j0k1l2"));
    }

    #[test]
    fn clean_summary_no_workspace_shows_mount_only() {
        let entries = vec![CleanEntry {
            workspace: None,
            mount: "dcx-old-thing-m3n4o5p6".to_string(),
            was: "empty dir".to_string(),
            action: "removed".to_string(),
        }];
        let out = format_clean_summary(&entries, 0);
        assert!(out.contains("dcx-old-thing-m3n4o5p6"));
        assert!(out.contains("was: empty dir"));
        assert!(out.contains("removed"));
        // The workspace→mount arrow must NOT appear (no workspace to display).
        assert!(!out.contains("None"));
        assert!(!out.contains("→  dcx-old-thing"));
    }

    // --- format_dry_run ---

    #[test]
    fn dry_run_empty_plans_shows_nothing_to_clean() {
        let out = format_dry_run(&[]);
        assert_eq!(out, "Nothing to clean.");
    }

    #[test]
    fn dry_run_single_running_container_shows_actions() {
        let plans = vec![DryRunPlan {
            mount_name: "dcx-myproject-a1b2c3d4".to_string(),
            state: "running".to_string(),
            container_id: Some("abc123def456".to_string()),
            runtime_image_id: Some("sha256:xyz".to_string()),
            has_base_image_tag: false,
            volumes: vec![],
            is_mounted: true,
        }];
        let out = format_dry_run(&plans);
        assert!(out.contains("Would clean:"), "got: {out}");
        assert!(out.contains("dcx-myproject-a1b2c3d4"), "got: {out}");
        assert!(out.contains("(running)"), "got: {out}");
        assert!(
            out.contains("Stop and remove container abc123def456"),
            "got: {out}"
        );
        assert!(
            out.contains("Remove runtime image sha256:xyz"),
            "got: {out}"
        );
        assert!(out.contains("Unmount bindfs"), "got: {out}");
        assert!(out.contains("Remove mount directory"), "got: {out}");
    }

    #[test]
    fn dry_run_with_purge_shows_build_image_and_volumes() {
        let plans = vec![DryRunPlan {
            mount_name: "dcx-myproject-a1b2c3d4".to_string(),
            state: "running".to_string(),
            container_id: Some("abc123".to_string()),
            runtime_image_id: Some("sha256:xyz".to_string()),
            has_base_image_tag: true,
            volumes: vec!["dcx-shellhistory-abc123".to_string()],
            is_mounted: true,
        }];
        let out = format_dry_run(&plans);
        assert!(out.contains("[purge]"), "missing [purge] marker");
        assert!(
            out.contains("Remove base image tag dcx-base:dcx-myproject-a1b2c3d4"),
            "got: {out}"
        );
        assert!(
            out.contains("Remove volume dcx-shellhistory-abc123"),
            "got: {out}"
        );
    }

    #[test]
    fn dry_run_orphaned_mount_shows_minimal_actions() {
        let plans = vec![DryRunPlan {
            mount_name: "dcx-old-e5f6g7h8".to_string(),
            state: "orphaned".to_string(),
            container_id: None,
            runtime_image_id: None,
            has_base_image_tag: false,
            volumes: vec![],
            is_mounted: true,
        }];
        let out = format_dry_run(&plans);
        assert!(out.contains("dcx-old-e5f6g7h8"), "got: {out}");
        assert!(out.contains("(orphaned)"), "got: {out}");
        assert!(
            !out.contains("Stop and remove container"),
            "should not stop container"
        );
        assert!(out.contains("Unmount bindfs"), "got: {out}");
    }

    #[test]
    fn dry_run_multiple_plans() {
        let plans = vec![
            DryRunPlan {
                mount_name: "dcx-project-a-a1b2c3d4".to_string(),
                state: "running".to_string(),
                container_id: Some("abc123".to_string()),
                runtime_image_id: None,
                has_base_image_tag: false,
                volumes: vec![],
                is_mounted: true,
            },
            DryRunPlan {
                mount_name: "dcx-project-b-e5f6g7h8".to_string(),
                state: "orphaned".to_string(),
                container_id: None,
                runtime_image_id: None,
                has_base_image_tag: false,
                volumes: vec![],
                is_mounted: false,
            },
        ];
        let out = format_dry_run(&plans);
        assert!(out.contains("dcx-project-a-a1b2c3d4"), "got: {out}");
        assert!(out.contains("dcx-project-b-e5f6g7h8"), "got: {out}");
        assert!(out.contains("(running)"), "got: {out}");
        assert!(out.contains("(orphaned)"), "got: {out}");
    }
}
