//! Project selection is data, independent of the JS runtime setting.

#[cfg(feature = "native-project")]
use semver::Version;
use std::str::FromStr;

/// Exact identity and bytes of a component, resolved by the host.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "native-project", derive(serde::Deserialize))]
#[cfg_attr(feature = "native-project", serde(deny_unknown_fields))]
pub struct BackendPackage {
    pub id: String,
    pub version: String,
    pub sha256: String,
    /// Relative to the project, or an explicit absolute development path.
    pub component: String,
}

/// An open backend/platform pair, such as `js/node` or `js/browser`.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "native-project", derive(serde::Deserialize))]
#[cfg_attr(feature = "native-project", serde(try_from = "String"))]
pub struct BackendEnvironment {
    pub backend: String,
    pub platform: String,
}

impl FromStr for BackendEnvironment {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let valid = |s: &str| {
            !s.is_empty()
                && s.bytes()
                    .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.'))
        };
        let (backend, platform) = value
            .split_once('/')
            .filter(|(b, p)| valid(b) && valid(p))
            .ok_or("an environment needs a backend and platform, such as `js/browser`")?;
        Ok(Self {
            backend: backend.into(),
            platform: platform.into(),
        })
    }
}

impl TryFrom<String> for BackendEnvironment {
    type Error = String;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        value.parse()
    }
}

#[cfg(feature = "native-project")]
impl BackendPackage {
    pub(super) fn validate(&self) -> Result<(), String> {
        let text = |s: &str| !s.is_empty() && s.trim() == s && !s.chars().any(char::is_control);
        if !text(&self.id) {
            return Err("a backend package needs a nonempty id without control characters".into());
        }
        Version::parse(&self.version)
            .map_err(|_| "a backend package needs an exact semantic version")?;
        if self.sha256.len() != 64
            || !self
                .sha256
                .bytes()
                .all(|b| b.is_ascii_digit() || matches!(b, b'a'..=b'f'))
        {
            return Err("a backend package needs a lowercase SHA-256 component digest".into());
        }
        if !text(&self.component) {
            return Err("a backend package needs a component path".into());
        }
        Ok(())
    }
}
