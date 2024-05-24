use std::fmt::{
    Debug,
    Display,
};

/// A value that can be either a definite `true` or `false`, or be indefinite.
///
/// # Comparisions
///
/// [`Maybe`]s can be compared (via [`PartialEq`]) with other [`Maybe`]s or
/// directly with [`bool`]s. If any value is [`Maybe::Indefinite`] the result of
/// the comparision is `false`.
#[derive(Clone, Copy)]
pub enum Maybe {
    Indefinite,
    Definite(bool),
}

impl Default for Maybe {
    /// The default value is [`Maybe::Indefinite`].
    fn default() -> Self {
        Self::Indefinite
    }
}

impl From<bool> for Maybe {
    fn from(value: bool) -> Self {
        Self::Definite(value)
    }
}

impl From<Option<bool>> for Maybe {
    fn from(value: Option<bool>) -> Self {
        value.map_or(Self::Indefinite, Self::Definite)
    }
}

impl From<Maybe> for Option<bool> {
    fn from(value: Maybe) -> Self {
        match value {
            Maybe::Definite(value) => Some(value),
            Maybe::Indefinite => None,
        }
    }
}

#[derive(Clone, Copy, Debug, thiserror::Error)]
#[error("Value is indefinite")]
pub struct IsIndefinite;

impl TryFrom<Maybe> for bool {
    type Error = IsIndefinite;

    fn try_from(value: Maybe) -> Result<Self, Self::Error> {
        Option::<bool>::from(value).ok_or(IsIndefinite)
    }
}

impl PartialEq for Maybe {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Definite(left), Self::Definite(right)) => left == right,
            _ => false,
        }
    }
}

impl PartialEq<bool> for Maybe {
    fn eq(&self, other: &bool) -> bool {
        match self {
            Self::Definite(left) => left == other,
            _ => false,
        }
    }
}

impl PartialEq<Maybe> for bool {
    fn eq(&self, other: &Maybe) -> bool {
        other.eq(self)
    }
}

impl Display for Maybe {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Indefinite => write!(f, "indefinite"),
            Self::Definite(value) => write!(f, "{value}"),
        }
    }
}

impl Debug for Maybe {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self}")
    }
}
