#![allow(dead_code)]

/// Exit code: success.
pub const SUCCESS: i32 = 0;

/// Exit code: runtime error (mount failed, devcontainer failed, etc.).
pub const RUNTIME_ERROR: i32 = 1;

/// Exit code: usage / input error (workspace doesn't exist, invalid args, etc.).
pub const USAGE_ERROR: i32 = 2;

/// Exit code: user aborted (answered "N" at confirmation prompt).
pub const USER_ABORTED: i32 = 4;

/// Exit code: prerequisite command not found (`bindfs`, `devcontainer`, etc.).
pub const PREREQ_NOT_FOUND: i32 = 127;
