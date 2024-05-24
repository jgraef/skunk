use std::{
    borrow::Cow,
    fmt::{
        Debug,
        Display,
    },
    hash::Hash,
    str::FromStr,
    sync::Arc,
};

use serde::{
    Deserialize,
    Serialize,
};

#[derive(Clone)]
pub struct Regex {
    string: Arc<str>,
    regex: regex::Regex,
}

#[derive(Debug, thiserror::Error)]
#[error("regex parse error")]
pub struct RegexParseError {
    #[source]
    source: regex::Error,
    string: Arc<str>,
}

impl FromStr for Regex {
    type Err = RegexParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Cow::Borrowed(s).try_into()
    }
}

impl<'a> TryFrom<Cow<'a, str>> for Regex {
    type Error = RegexParseError;

    fn try_from(string: Cow<'a, str>) -> Result<Self, Self::Error> {
        Arc::<str>::from(string).try_into()
    }
}

impl TryFrom<String> for Regex {
    type Error = RegexParseError;

    fn try_from(string: String) -> Result<Self, Self::Error> {
        Arc::<str>::from(string).try_into()
    }
}

impl<'a> TryFrom<&'a str> for Regex {
    type Error = RegexParseError;

    fn try_from(string: &'a str) -> Result<Self, Self::Error> {
        Arc::<str>::from(string).try_into()
    }
}

impl TryFrom<Arc<str>> for Regex {
    type Error = RegexParseError;

    fn try_from(string: Arc<str>) -> Result<Self, Self::Error> {
        match string.parse() {
            Ok(regex) => Ok(Self { string, regex }),
            Err(e) => Err(RegexParseError { source: e, string }),
        }
    }
}

impl PartialEq for Regex {
    fn eq(&self, other: &Self) -> bool {
        self.string == other.string
    }
}

impl Eq for Regex {}

impl Hash for Regex {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.string.hash(state);
    }
}

impl Display for Regex {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.string)
    }
}

impl Debug for Regex {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self.string)
    }
}

impl Serialize for Regex {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        self.string.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for Regex {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s: Cow<'de, str> = Deserialize::deserialize(deserializer)?;
        s.try_into().map_err(serde::de::Error::custom)
    }
}
