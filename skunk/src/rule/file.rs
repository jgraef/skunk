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

use ip_network::IpNetwork;
use serde::{
    Deserialize,
    Serialize,
};

use super::regex::Regex;
use crate::address::Ports;

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RulesFile<F = DefaultFilters, E = DefaultEffects> {
    #[serde(default, skip_serializing_if = "Metadata::is_empty")]
    pub meta: Metadata,

    #[serde(default = "Default::default", flatten)]
    pub rules: Block<F, E>,
}

impl<F, E> Default for RulesFile<F, E> {
    fn default() -> Self {
        Self {
            meta: Default::default(),
            rules: Default::default(),
        }
    }
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

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Block<F, E> {
    #[serde(default = "Vec::new", skip_serializing_if = "Vec::is_empty")]
    pub rules: Vec<Rule<F, E>>,

    #[serde(default = "Vec::new", skip_serializing_if = "Vec::is_empty")]
    pub effects: Vec<E>,
}

impl<F, E> Default for Block<F, E> {
    fn default() -> Self {
        Self {
            rules: vec![],
            effects: vec![],
        }
    }
}

impl<F, E> Block<F, E> {
    pub fn is_empty(&self) -> bool {
        self.rules.is_empty() && self.effects.is_empty()
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Rule<F, E> {
    #[serde(
        rename = "if",
        default = "Default::default",
        skip_serializing_if = "Conditions::is_empty"
    )]
    pub condition: Conditions<F>,

    #[serde(default = "Default::default", skip_serializing_if = "Block::is_empty")]
    pub then: Block<F, E>,

    #[serde(
        rename = "else",
        default = "Default::default",
        skip_serializing_if = "Block::is_empty"
    )]
    pub alt: Block<F, E>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Conditions<F>(pub Vec<Condition<F>>);

impl<F> Default for Conditions<F> {
    fn default() -> Self {
        Self(vec![])
    }
}

impl<F> Conditions<F> {
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Condition<F> {
    Sub(SubCondition<F>),
    Terminal(F),
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SubCondition<F> {
    Not(Conditions<F>),
    And(Conditions<F>),
    Or(Conditions<F>),
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum DefaultFilters {
    Direction(Direction),
    Host(Vec<Regex>),
    Tcp(Vec<TcpFilter>),
    Tls(Vec<TlsFilter>),
    Http(Vec<HttpFilter>),
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Direction {
    Request,
    Response,
    Both,
}

impl Default for Direction {
    fn default() -> Self {
        Self::Both
    }
}

impl Direction {
    pub fn is_both(&self) -> bool {
        matches!(self, Self::Both)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TcpFilter {
    HostPort(Vec<HostPort>),
    Hostname(Vec<Regex>),
    DnsName(Vec<Regex>),
    IpAddress(Vec<IpNetwork>),
    Port(Vec<Ports>),
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HostPort {
    host: Regex,
    port: Ports,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TlsFilter {
    ServerName(Vec<Regex>),
    #[serde(alias = "cn")]
    CommonName(Vec<Regex>),
    #[serde(alias = "dn")]
    DistinguishedName(Vec<Regex>),
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
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
#[serde(rename_all = "kebab-case")]
pub enum DefaultEffects {
    Log(LogEffect),
    Interrupt(InterruptEffect),
    Drop,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct LogEffect {
    #[serde(skip_serializing_if = "LogTarget::is_user")]
    target: LogTarget,

    #[serde(skip_serializing_if = "Option::is_none")]
    name: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    message: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
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

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct InterruptEffect {
    #[serde(skip_serializing_if = "Option::is_none")]
    prompt: Option<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("io error")]
    Io(#[from] std::io::Error),

    #[error("yaml error")]
    Yaml(#[from] serde_yml::Error),
}

pub fn from_reader<F, E>(reader: impl Read) -> Result<RulesFile<F, E>, Error>
where
    F: for<'de> Deserialize<'de>,
    E: for<'de> Deserialize<'de>,
{
    Ok(serde_yml::from_reader(reader)?)
}

pub fn from_file<F, E>(path: impl AsRef<Path>) -> Result<RulesFile<F, E>, Error>
where
    F: for<'de> Deserialize<'de>,
    E: for<'de> Deserialize<'de>,
{
    let file = File::open(path)?;
    let reader = BufReader::new(file);
    from_reader(reader)
}

pub fn to_writer<F, E>(writer: impl Write, rules: &RulesFile<F, E>) -> Result<(), Error>
where
    F: Serialize,
    E: Serialize,
{
    serde_yml::to_writer(writer, rules)?;
    Ok(())
}

pub fn to_file<F, E>(path: impl AsRef<Path>, rules: &RulesFile<F, E>) -> Result<(), Error>
where
    F: Serialize,
    E: Serialize,
{
    let file = File::create(path)?;
    let writer = BufWriter::new(file);
    to_writer(writer, rules)
}
