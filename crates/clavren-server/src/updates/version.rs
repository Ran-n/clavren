// -----------------------------------------------------------------------------
//+ Authors: 	Ran# <ran.hash@proton.me>
//+ Created: 	2026/11/09 09:57:45.752530
//+ Revised: 	2026/11/09 16:05:28.067826
// -----------------------------------------------------------------------------

//! A release version: three plain numbers, compared as numbers (0.10.0 is newer than 0.9.0). No
//! prefix, no pre-release suffix and no leading zeros, so a version has exactly one spelling.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct Version(pub u64, pub u64, pub u64);

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
#[error("{0:?} is not a version: expected three plain numbers such as 1.2.3")]
pub struct InvalidVersion(String);

impl Version {
    /// The version of this build.
    pub fn running() -> Self {
        env!("CARGO_PKG_VERSION").parse().unwrap_or_else(|error| panic!("the crate version must parse: {error}"))
    }
}

impl FromStr for Version {
    type Err = InvalidVersion;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        let invalid = || InvalidVersion(text.to_string());
        let mut parts = text.split('.').map(|part| {
            let plain = !part.is_empty() && part.bytes().all(|byte| byte.is_ascii_digit());
            let leading_zero = part.len() > 1 && part.starts_with('0');
            if plain && !leading_zero { part.parse::<u64>().map_err(|_| invalid()) } else { Err(invalid()) }
        });
        let (Some(major), Some(minor), Some(patch), None) = (parts.next(), parts.next(), parts.next(), parts.next())
        else {
            return Err(invalid());
        };
        Ok(Self(major?, minor?, patch?))
    }
}

impl fmt::Display for Version {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}.{}.{}", self.0, self.1, self.2)
    }
}

impl TryFrom<String> for Version {
    type Error = InvalidVersion;

    fn try_from(text: String) -> Result<Self, Self::Error> {
        text.parse()
    }
}

impl From<Version> for String {
    fn from(version: Version) -> Self {
        version.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn version(text: &str) -> Version {
        text.parse().unwrap()
    }

    #[test]
    fn versions_compare_as_numbers() {
        assert!(version("0.10.0") > version("0.9.0"));
        assert!(version("1.0.0") > version("0.99.99"));
        assert!(version("0.3.1") > version("0.3.0"));
        assert_eq!(version("0.3.0"), version("0.3.0"));
        assert_eq!(version("12.0.7"), Version(12, 0, 7));
    }

    #[test]
    fn only_three_plain_numbers_are_versions() {
        for text in
            ["v1.2.3", "1.2", "0.1.2.3", "1.2.3-rc1", "01.2.3", "1.02.3", "1..3", "", "1.2.x", " 1.2.3", "+1.2.3"]
        {
            assert_eq!(text.parse::<Version>(), Err(InvalidVersion(text.to_string())), "{text:?}");
        }
        assert_eq!(version("0.0.0"), Version(0, 0, 0));
    }

    #[test]
    fn versions_read_and_write_as_text() {
        assert_eq!(serde_json::to_string(&Version(0, 4, 0)).unwrap(), "\"0.4.0\"");
        assert_eq!(serde_json::from_str::<Version>("\"0.10.2\"").unwrap(), Version(0, 10, 2));
        assert!(serde_json::from_str::<Version>("\"v0.10.2\"").is_err());
    }

    #[test]
    fn this_build_has_a_version() {
        assert_eq!(Version::running().to_string(), env!("CARGO_PKG_VERSION"));
    }
}
