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
    fn format_step_includes_arrow() {
        let formatted = format_step("hello");
        assert!(
            formatted.contains("→"),
            "format_step should include the arrow character"
        );
    }

    #[test]
    fn format_step_includes_message() {
        let formatted = format_step("test message");
        assert!(
            formatted.contains("test message"),
            "format_step should include the message"
        );
    }

    #[test]
    fn format_step_arrow_before_message() {
        let formatted = format_step("hello");
        let arrow_idx = formatted.find('→');
        let msg_idx = formatted.find("hello");
        assert!(arrow_idx.is_some() && msg_idx.is_some());
        assert!(
            arrow_idx.unwrap() < msg_idx.unwrap(),
            "arrow should come before message"
        );
    }

    #[test]
    fn format_step_handles_empty_message() {
        let formatted = format_step("");
        assert!(
            formatted.contains("→"),
            "format_step should include arrow even for empty message"
        );
    }
}
