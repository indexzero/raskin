//! Built-in selections and owner-defined exact-key capture presets.
use std::collections::{BTreeMap, BTreeSet};

use crate::codec::Decoder;
use crate::model::MAX_DEPTH;
use crate::presets::Preset;
use crate::{Domain, Host, Result, Scope};
use kdl::{KdlNode, KdlValue};

/// A validated selection of exact top-level keys, with no preference values.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FilePreset {
    pub(crate) domains: BTreeMap<Domain, BTreeSet<String>>,
    pub(crate) power: BTreeMap<String, BTreeSet<String>>,
}

impl FilePreset {
    /// Parse a `raskin-preset 1` document without reading any preference sources.
    pub fn decode(input: &str, source_name: &str) -> Result<Self> {
        let decoder = Decoder { input, source_name };
        let tree = decoder.tree()?;
        let mut preset = Self::default();
        let mut roots = BTreeSet::new();
        for node in tree.nodes() {
            let name = node.name().value();
            if !roots.insert(name) {
                return Err(decoder.error(node, format!("duplicate root group {name:?}")));
            }
            match name {
                "raskin-preset" => {
                    decoder.no_annotation(node)?;
                    if node.entries().len() != 1
                        || node.children().is_some()
                        || node.entries()[0].name().is_some()
                        || node.entries()[0].ty().is_some()
                        || node.entries()[0].value() != &KdlValue::Integer(1)
                    {
                        return Err(decoder.error(node, "expected exactly raskin-preset 1"));
                    }
                }
                "defaults" => {
                    decoder.group(node)?;
                    for child in node.children().expect("validated group").nodes() {
                        preset.namespace(&decoder, child, "")?;
                    }
                }
                "pmset" => {
                    decoder.group(node)?;
                    let mut profiles = BTreeSet::new();
                    for profile in node.children().expect("validated group").nodes() {
                        decoder.group(profile)?;
                        let name = profile.name().value();
                        if name.is_empty() || !profiles.insert(name) {
                            return Err(decoder.error(profile, "empty or duplicate power profile"));
                        }
                        let mut keys = BTreeSet::new();
                        for key in profile.children().expect("validated group").nodes() {
                            insert_key(&decoder, &mut keys, key)?;
                        }
                        if !keys.is_empty() {
                            preset.power.insert(name.to_owned(), keys);
                        }
                    }
                }
                _ => return Err(decoder.error(node, format!("unknown preset group {name:?}"))),
            }
        }
        if !roots.contains("raskin-preset") {
            return Err(decoder.at(0, "missing raskin-preset 1 schema header"));
        }
        Ok(preset)
    }

    pub fn domains(&self) -> &BTreeMap<Domain, BTreeSet<String>> {
        &self.domains
    }

    pub fn power(&self) -> &BTreeMap<String, BTreeSet<String>> {
        &self.power
    }

    fn namespace(&mut self, decoder: &Decoder<'_>, node: &KdlNode, prefix: &str) -> Result<()> {
        let domain = decoder.domain(node, prefix)?;
        if domain.name.split('.').any(str::is_empty) || domain.name.split('.').count() >= MAX_DEPTH
        {
            return Err(decoder.error(node, "empty domain segment or excessive namespace depth"));
        }
        let mut keys = BTreeSet::new();
        let mut branches = Vec::new();
        for child in node.children().expect("validated namespace").nodes() {
            if child.children().is_some() {
                branches.push(child);
            } else {
                insert_key(decoder, &mut keys, child)?;
            }
        }
        if keys.is_empty() && !node.entries().is_empty() {
            return Err(decoder.error(node, "scope belongs on a domain with direct selected keys"));
        }
        let name = domain.name.clone();
        if !keys.is_empty() && self.domains.insert(domain, keys).is_some() {
            return Err(decoder.error(node, format!("duplicate scoped domain {name:?}")));
        }
        for child in branches {
            self.namespace(decoder, child, &name)?;
        }
        Ok(())
    }
}

fn insert_key(decoder: &Decoder<'_>, keys: &mut BTreeSet<String>, node: &KdlNode) -> Result<()> {
    if node.ty().is_some() || !node.entries().is_empty() || node.children().is_some() {
        return Err(decoder.error(
            node,
            "preset keys must have no values, properties, type annotations or children",
        ));
    }
    if !keys.insert(node.name().value().to_owned()) {
        return Err(decoder.error(node, "duplicate selected key"));
    }
    Ok(())
}

/// Capture accepts the existing built-in presets or a parsed exact-key file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Selection {
    BuiltIn(Preset),
    File(FilePreset),
}

impl From<Preset> for Selection {
    fn from(value: Preset) -> Self {
        Self::BuiltIn(value)
    }
}
impl From<FilePreset> for Selection {
    fn from(value: FilePreset) -> Self {
        Self::File(value)
    }
}

impl Selection {
    pub(crate) fn is_broad(&self) -> bool {
        matches!(self, Self::BuiltIn(Preset::Broad))
    }

    pub(crate) fn includes_store(
        &self,
        scope: Scope,
        container: Option<&str>,
        host: Option<Host>,
    ) -> bool {
        match self {
            Self::BuiltIn(preset) => {
                preset.includes_preferences()
                    && container.is_none_or(|id| preset.includes_container(id))
            }
            Self::File(file) => file.domains.keys().any(|domain| {
                domain.scope == scope
                    && domain.container.as_deref() == container
                    && host.is_none_or(|host| domain.host == host)
            }),
        }
    }

    pub(crate) fn includes_containers(&self) -> bool {
        match self {
            Self::BuiltIn(preset) => preset.includes_preferences(),
            Self::File(file) => file.domains.keys().any(|domain| domain.container.is_some()),
        }
    }

    pub(crate) fn includes_container(&self, id: &str) -> bool {
        match self {
            Self::BuiltIn(preset) => preset.includes_container(id),
            Self::File(file) => file
                .domains
                .keys()
                .any(|domain| domain.container.as_deref() == Some(id)),
        }
    }

    pub(crate) fn includes_source(&self, domain: &Domain) -> bool {
        match self {
            Self::BuiltIn(preset) => {
                preset.includes_source(&domain.name, domain.container.as_deref())
            }
            Self::File(file) => file.domains.contains_key(domain),
        }
    }

    pub(crate) fn includes_key(&self, domain: &Domain, key: &str) -> bool {
        match self {
            Self::BuiltIn(_) => true,
            Self::File(file) => file
                .domains
                .get(domain)
                .is_some_and(|keys| keys.contains(key)),
        }
    }

    pub(crate) fn includes_power(&self) -> bool {
        match self {
            Self::BuiltIn(preset) => preset.includes_power(),
            Self::File(file) => !file.power.is_empty(),
        }
    }

    pub(crate) fn includes_profile(&self, profile: &str) -> bool {
        match self {
            Self::BuiltIn(_) => true,
            Self::File(file) => file.power.contains_key(profile),
        }
    }

    pub(crate) fn includes_power_key(&self, profile: &str, key: &str) -> bool {
        match self {
            Self::BuiltIn(_) => true,
            Self::File(file) => file
                .power
                .get(profile)
                .is_some_and(|keys| keys.contains(key)),
        }
    }
}
