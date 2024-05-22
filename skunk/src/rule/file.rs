use std::{
    fs::File,
    io::{
        BufReader,
        BufWriter,
        Read,
        Write,
    },
    path::Path,
};

use serde::{
    Deserialize,
    Serialize,
};

use super::regex::Regex;
use crate::address::Ports;

#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RulesFile {
    #[serde(default, skip_serializing_if = "Metadata::is_empty")]
    pub meta: Metadata,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub rules: Vec<Rule>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Metadata {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,

    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub user_interaction: bool,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,

    #[serde(flatten)]
    pub other: serde_yml::Mapping,
}

impl Metadata {
    pub fn is_empty(&self) -> bool {
        self.author.is_none()
            && self.name.is_none()
            && self.description.is_none()
            && self.tags.is_empty()
            && !self.user_interaction
            && self.version.is_none()
            && self.other.is_empty()
    }
}

#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Rule {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub filters: Vec<Filter>,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub effects: Vec<Effect>,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub and_then: Vec<Rule>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum Filter {
    Direction(Direction),
    Tcp(Vec<TcpFilter>),
    Tls(Vec<TlsFilter>),
    Http(Vec<HttpFilter>),
    Host(Vec<Regex>),
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Direction {
    Request,
    Response,
    Both,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TcpFilter {
    Host(Vec<Regex>),
    Port(Vec<Ports>),
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum TlsFilter {
    ServerName(Vec<Regex>),
    #[serde(alias = "cn")]
    CommonName(Vec<Regex>),
    #[serde(alias = "dn")]
    DistinguishedName(Vec<Regex>),
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum HttpFilter {
    Method(Vec<Regex>),
    //StatusCode(u16),
    Url(Vec<Regex>),
    Header { name: Regex, value: Regex },
    ContentType(Vec<Regex>),
    Cookie(Vec<Regex>),
    Host(Vec<Regex>),
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum Effect {
    Log {
        #[serde(skip_serializing_if = "LogTarget::is_user")]
        target: LogTarget,

        #[serde(skip_serializing_if = "Option::is_none")]
        name: Option<String>,

        #[serde(skip_serializing_if = "Option::is_none")]
        message: Option<String>,
    },
    Interrupt {
        #[serde(skip_serializing_if = "Option::is_none")]
        prompt: Option<String>,
    },
    Drop,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LogTarget {
    User,
    File,
}

impl LogTarget {
    pub fn is_user(&self) -> bool {
        matches!(self, Self::User)
    }

    pub fn is_file(&self) -> bool {
        matches!(self, Self::File)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("io error")]
    Io(#[from] std::io::Error),

    #[error("yaml error")]
    Yaml(#[from] serde_yml::Error),
}

pub fn from_reader(reader: impl Read) -> Result<RulesFile, Error> {
    Ok(serde_yml::from_reader(reader)?)
}

pub fn from_file(path: impl AsRef<Path>) -> Result<RulesFile, Error> {
    let file = File::open(path)?;
    let reader = BufReader::new(file);
    from_reader(reader)
}

pub fn to_writer(writer: impl Write, rules: &RulesFile) -> Result<(), Error> {
    serde_yml::to_writer(writer, rules)?;
    Ok(())
}

pub fn to_file(path: impl AsRef<Path>, rules: &RulesFile) -> Result<(), Error> {
    let file = File::create(path)?;
    let writer = BufWriter::new(file);
    to_writer(writer, rules)
}
