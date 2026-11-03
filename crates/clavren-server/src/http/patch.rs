// -----------------------------------------------------------------------------
//+ Authors: 	Ran# <ran.hash@proton.me>
//+ Created: 	2026/11/03 04:54:15.016507
//+ Revised: 	2026/11/03 04:54:15.016507
// -----------------------------------------------------------------------------

//! One field of a `PATCH` body. JSON has three cases and the API keeps them apart: the field is
//! absent (leave it as it is), `null` (clear it), or a value (set it).

use serde::{Deserialize, Deserializer};

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum Patch<T> {
    #[default]
    Keep,
    Clear,
    Set(T),
}

impl<T> Patch<T> {
    /// The new value given the current one.
    pub fn apply(self, current: Option<T>) -> Option<T> {
        match self {
            Self::Keep => current,
            Self::Clear => None,
            Self::Set(value) => Some(value),
        }
    }

    pub fn is_keep(&self) -> bool {
        matches!(self, Self::Keep)
    }
}

/// Only `null` or a value reach this; an absent field takes the `Keep` default (the containing
/// struct needs `#[serde(default)]`).
impl<'de, T: Deserialize<'de>> Deserialize<'de> for Patch<T> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Option::<T>::deserialize(deserializer).map(|value| value.map_or(Self::Clear, Self::Set))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Deserialize, Default)]
    #[serde(default)]
    struct Body {
        notes: Patch<String>,
    }

    fn parse(json: &str) -> Patch<String> {
        serde_json::from_str::<Body>(json).unwrap().notes
    }

    #[test]
    fn absent_null_and_a_value_are_told_apart() {
        assert_eq!(parse("{}"), Patch::Keep);
        assert_eq!(parse(r#"{"notes":null}"#), Patch::Clear);
        assert_eq!(parse(r#"{"notes":"hi"}"#), Patch::Set("hi".into()));
    }

    #[test]
    fn applying_gives_the_new_value() {
        let current = Some("old".to_string());
        assert_eq!(Patch::Keep.apply(current.clone()), current);
        assert_eq!(Patch::<String>::Clear.apply(current.clone()), None);
        assert_eq!(Patch::Set("new".to_string()).apply(current), Some("new".into()));
        assert!(Patch::<u8>::Keep.is_keep() && !Patch::Set(1).is_keep());
    }
}
