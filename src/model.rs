//! Typed preference data. Collection order is canonical; array order is data.
use crate::{Error, Result};
use std::collections::BTreeMap;

pub const MAX_DEPTH: usize = 128;
pub const MAX_DOCUMENT_BYTES: usize = 128 * 1024 * 1024;
pub type Properties = BTreeMap<String, Setting>;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub enum Scope {
    #[default]
    User,
    System,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub enum Host {
    #[default]
    Any,
    Current,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Domain {
    pub name: String,
    pub scope: Scope,
    pub host: Host,
    pub container: Option<String>,
}
impl Domain {
    pub fn user(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            scope: Scope::User,
            host: Host::Any,
            container: None,
        }
    }
    pub fn identity(&self) -> String {
        format!(
            "defaults:{:?}[scope={:?},host={:?},container={:?}]",
            self.name, self.scope, self.host, self.container
        )
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Document {
    pub domains: BTreeMap<Domain, Properties>,
    pub power: BTreeMap<String, Properties>,
    pub unavailable: Vec<Unavailable>,
}
impl Document {
    /// Validate library-created values before encoding or rendering.
    pub fn validate(&self) -> Result<()> {
        for (domain, properties) in &self.domains {
            if properties.is_empty() {
                return Err(Error::Invalid(format!(
                    "domain {:?} has no preference values",
                    domain.name
                )));
            }
            if domain.name.is_empty() || domain.name.split('.').any(str::is_empty) {
                return Err(Error::Invalid(format!(
                    "empty domain segment in {:?}",
                    domain.name
                )));
            }
            if domain.name.split('.').count() >= MAX_DEPTH {
                return Err(Error::Invalid("domain exceeds nesting limit".into()));
            }
            for setting in properties.values() {
                setting.value.validate(domain.name.split('.').count() + 1)?;
            }
        }
        for (profile, properties) in &self.power {
            if profile.is_empty() {
                return Err(Error::Invalid("empty power profile".into()));
            }
            for setting in properties.values() {
                if !setting.value.is_scalar() {
                    return Err(Error::Invalid(
                        "power settings require scalar values".into(),
                    ));
                }
                setting.value.validate(0)?;
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug)]
pub struct Setting {
    pub value: Value,
    pub metadata: BTreeMap<String, kdl::KdlValue>,
}
impl Setting {
    pub fn new(value: Value) -> Self {
        Self {
            value,
            metadata: BTreeMap::new(),
        }
    }
}
impl From<Value> for Setting {
    fn from(value: Value) -> Self {
        Self::new(value)
    }
}
impl PartialEq for Setting {
    fn eq(&self, other: &Self) -> bool {
        self.value == other.value
            && self.metadata.len() == other.metadata.len()
            && self.metadata.iter().all(|(key, value)| {
                other
                    .metadata
                    .get(key)
                    .is_some_and(|right| match (value, right) {
                        (kdl::KdlValue::Float(a), kdl::KdlValue::Float(b)) => real_eq(*a, *b),
                        _ => value == right,
                    })
            })
    }
}

#[derive(Clone, Debug)]
pub enum Value {
    Boolean(bool),
    Integer(i128),
    Real(f64),
    String(String),
    Date(plist::Date),
    Data(Vec<u8>),
    Uid(u64),
    Array(Vec<Setting>),
    Dict(Properties),
}
fn real_eq(a: f64, b: f64) -> bool {
    a.to_bits() == b.to_bits() || (a.is_nan() && b.is_nan())
}
impl PartialEq for Value {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Boolean(a), Self::Boolean(b)) => a == b,
            (Self::Integer(a), Self::Integer(b)) => a == b,
            (Self::Real(a), Self::Real(b)) => real_eq(*a, *b),
            (Self::String(a), Self::String(b)) => a == b,
            (Self::Date(a), Self::Date(b)) => a == b,
            (Self::Data(a), Self::Data(b)) => a == b,
            (Self::Uid(a), Self::Uid(b)) => a == b,
            (Self::Array(a), Self::Array(b)) => a == b,
            (Self::Dict(a), Self::Dict(b)) => a == b,
            _ => false,
        }
    }
}
impl Value {
    pub fn is_scalar(&self) -> bool {
        !matches!(self, Self::Array(_) | Self::Dict(_))
    }
    pub fn validate(&self, depth: usize) -> Result<()> {
        // Empty collections still need a child block, which also counts
        // toward the decoder's limit on nested KDL braces.
        if depth > MAX_DEPTH || (depth == MAX_DEPTH && !self.is_scalar()) {
            return Err(Error::Invalid("value exceeds nesting limit of 128".into()));
        }
        match self {
            Self::Date(date) => {
                let time: std::time::SystemTime = (*date).into();
                let earliest = std::time::UNIX_EPOCH
                    .checked_sub(std::time::Duration::from_secs(62_167_219_200));
                let latest = std::time::UNIX_EPOCH
                    .checked_add(std::time::Duration::from_secs(253_402_300_800));
                if earliest.is_none_or(|min| time < min) || latest.is_none_or(|max| time >= max) {
                    Err(Error::Invalid(
                        "date is outside the supported RFC 3339 year range".into(),
                    ))
                } else {
                    Ok(())
                }
            }
            Self::Integer(n) if *n < i64::MIN as i128 || *n > u64::MAX as i128 => Err(
                Error::Invalid("integer is outside plist range (-2^63 through 2^64-1)".into()),
            ),
            Self::Array(items) => {
                for s in items {
                    s.value.validate(depth + 1)?;
                }
                Ok(())
            }
            Self::Dict(items) => {
                for s in items.values() {
                    s.value.validate(depth + 1)?;
                }
                Ok(())
            }
            _ => Ok(()),
        }
    }
    pub fn from_plist(value: plist::Value) -> Result<Self> {
        Self::from_plist_inner(value, 0)
    }
    fn from_plist_inner(value: plist::Value, depth: usize) -> Result<Self> {
        if depth > MAX_DEPTH {
            return Err(Error::Invalid("plist exceeds nesting limit of 128".into()));
        }
        Ok(match value {
            plist::Value::Boolean(v) => Self::Boolean(v),
            plist::Value::Integer(v) => Self::Integer(if let Some(n) = v.as_signed() {
                n as i128
            } else {
                v.as_unsigned()
                    .ok_or_else(|| Error::Invalid("unsupported plist integer".into()))?
                    as i128
            }),
            plist::Value::Real(v) => Self::Real(v),
            plist::Value::String(v) => Self::String(v),
            plist::Value::Date(v) => Self::Date(v),
            plist::Value::Data(v) => Self::Data(v),
            plist::Value::Uid(v) => Self::Uid(v.get()),
            plist::Value::Array(items) => Self::Array(
                items
                    .into_iter()
                    .map(|v| Self::from_plist_inner(v, depth + 1).map(Setting::new))
                    .collect::<Result<_>>()?,
            ),
            plist::Value::Dictionary(items) => Self::Dict(
                items
                    .into_iter()
                    .map(|(k, v)| Ok((k, Setting::new(Self::from_plist_inner(v, depth + 1)?))))
                    .collect::<Result<_>>()?,
            ),
            _ => return Err(Error::Invalid("unsupported native plist type".into())),
        })
    }
    pub fn to_plist(&self) -> Result<plist::Value> {
        self.validate(0)?;
        Ok(match self {
            Self::Boolean(v) => plist::Value::Boolean(*v),
            Self::Integer(v) if *v < 0 => plist::Value::Integer((*v as i64).into()),
            Self::Integer(v) => plist::Value::Integer((*v as u64).into()),
            Self::Real(v) => plist::Value::Real(*v),
            Self::String(v) => plist::Value::String(v.clone()),
            Self::Date(v) => plist::Value::Date(*v),
            Self::Data(v) => plist::Value::Data(v.clone()),
            Self::Uid(v) => plist::Value::Uid(plist::Uid::new(*v)),
            Self::Array(items) => plist::Value::Array(
                items
                    .iter()
                    .map(|v| v.value.to_plist())
                    .collect::<Result<_>>()?,
            ),
            Self::Dict(items) => plist::Value::Dictionary(
                items
                    .iter()
                    .map(|(k, v)| Ok((k.clone(), v.value.to_plist()?)))
                    .collect::<Result<_>>()?,
            ),
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Reason {
    PermissionDenied,
    Unsupported,
    Missing,
    ReadFailed,
}
impl Reason {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::PermissionDenied => "permission-denied",
            Self::Unsupported => "unsupported",
            Self::Missing => "missing",
            Self::ReadFailed => "read-failed",
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Unavailable {
    pub source: String,
    pub reason: Reason,
}

/// Keys verified in pmset(1) or Apple's pmset source; unknown/read-only fields never emit commands.
pub fn power_key_supported(profile: &str, key: &str) -> bool {
    if !matches!(profile, "battery" | "charger" | "ups") {
        return false;
    }
    match key {
        "haltlevel" | "haltafter" | "haltremain" => profile == "ups",
        "displaysleep"
        | "disksleep"
        | "sleep"
        | "womp"
        | "ring"
        | "powernap"
        | "proximitywake"
        | "autorestart"
        | "autorestartatconnect"
        | "lidwake"
        | "acwake"
        | "lessbright"
        | "halfdim"
        | "sms"
        | "hibernatemode"
        | "ttyskeepawake"
        | "tcpkeepalive"
        | "destroyfvkeyonstandby"
        | "standby"
        | "standbydelayhigh"
        | "standbydelaylow"
        | "highstandbythreshold"
        | "autopoweroff"
        | "autopoweroffdelay" => true,
        _ => false,
    }
}
