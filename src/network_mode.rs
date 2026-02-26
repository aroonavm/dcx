use std::fmt;
use std::str::FromStr;

/// Network isolation level for devcontainer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum NetworkMode {
    /// No network access (DROP all traffic)
    Restricted,
    /// Dev tools only: GitHub, npm, Anthropic (default)
    #[default]
    Minimal,
    /// Host network only
    Host,
    /// Unrestricted access
    Open,
}

impl FromStr for NetworkMode {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "restricted" => Ok(Self::Restricted),
            "minimal" => Ok(Self::Minimal),
            "host" => Ok(Self::Host),
            "open" => Ok(Self::Open),
            _ => Err(format!(
                "Invalid network mode '{}'. Must be one of: restricted, minimal, host, open",
                s
            )),
        }
    }
}

impl fmt::Display for NetworkMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Restricted => write!(f, "restricted"),
            Self::Minimal => write!(f, "minimal"),
            Self::Host => write!(f, "host"),
            Self::Open => write!(f, "open"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_restricted() {
        assert_eq!(
            "restricted".parse::<NetworkMode>().unwrap(),
            NetworkMode::Restricted
        );
        assert_eq!(
            "RESTRICTED".parse::<NetworkMode>().unwrap(),
            NetworkMode::Restricted
        );
    }

    #[test]
    fn parse_minimal() {
        assert_eq!(
            "minimal".parse::<NetworkMode>().unwrap(),
            NetworkMode::Minimal
        );
        assert_eq!(
            "MINIMAL".parse::<NetworkMode>().unwrap(),
            NetworkMode::Minimal
        );
    }

    #[test]
    fn parse_host() {
        assert_eq!("host".parse::<NetworkMode>().unwrap(), NetworkMode::Host);
        assert_eq!("HOST".parse::<NetworkMode>().unwrap(), NetworkMode::Host);
    }

    #[test]
    fn parse_open() {
        assert_eq!("open".parse::<NetworkMode>().unwrap(), NetworkMode::Open);
        assert_eq!("OPEN".parse::<NetworkMode>().unwrap(), NetworkMode::Open);
    }

    #[test]
    fn parse_invalid() {
        assert!("invalid".parse::<NetworkMode>().is_err());
        assert!("".parse::<NetworkMode>().is_err());
    }

    #[test]
    fn display_format() {
        assert_eq!(NetworkMode::Restricted.to_string(), "restricted");
        assert_eq!(NetworkMode::Minimal.to_string(), "minimal");
        assert_eq!(NetworkMode::Host.to_string(), "host");
        assert_eq!(NetworkMode::Open.to_string(), "open");
    }

    #[test]
    fn default_is_minimal() {
        assert_eq!(NetworkMode::default(), NetworkMode::Minimal);
    }

    #[test]
    fn display_round_trip() {
        let modes = [
            NetworkMode::Restricted,
            NetworkMode::Minimal,
            NetworkMode::Host,
            NetworkMode::Open,
        ];
        for mode in modes {
            assert_eq!(mode.to_string().parse::<NetworkMode>().unwrap(), mode);
        }
    }
}
