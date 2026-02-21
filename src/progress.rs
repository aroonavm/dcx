#![allow(dead_code)]

/// Format a progress step message: `→ <message>`.
///
/// The arrow is U+2192 (→), matching the spec's progress output format.
pub fn format_step(msg: &str) -> String {
    format!("\u{2192} {msg}")
}

/// Print a progress step to stderr: `→ <message>`.
pub fn step(msg: &str) {
    eprintln!("{}", format_step(msg));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_step_exact_format() {
        assert_eq!(format_step("test"), "\u{2192} test");
    }
}
