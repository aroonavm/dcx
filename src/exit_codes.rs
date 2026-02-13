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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn success_is_zero() {
        assert_eq!(SUCCESS, 0);
    }

    #[test]
    fn runtime_error_is_one() {
        assert_eq!(RUNTIME_ERROR, 1);
    }

    #[test]
    fn usage_error_is_two() {
        assert_eq!(USAGE_ERROR, 2);
    }

    #[test]
    fn user_aborted_is_four() {
        assert_eq!(USER_ABORTED, 4);
    }

    #[test]
    fn prereq_not_found_is_127() {
        assert_eq!(PREREQ_NOT_FOUND, 127);
    }
}
