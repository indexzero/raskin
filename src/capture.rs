//! Read persisted settings through an isolated, mockable read boundary.
//!
//! Filesystem adapters refuse observed symlinks in every path component. This is
//! not an adversarial filesystem sandbox: a concurrent replacement between the
//! metadata check and open is outside this initial adapter's guarantees.
use std::collections::{BTreeMap, BTreeSet};
use std::ffi::OsString;
use std::fmt;
use std::fs;
use std::io::{self, Cursor, Read};
use std::path::{Component, Path, PathBuf};
use std::str::FromStr;

use crate::model::{MAX_DEPTH, MAX_DOCUMENT_BYTES, power_key_supported};
use crate::{
    Document, Domain, Error, Host, Properties, Reason, Result, Scope, Setting, Unavailable, Value,
};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Preset {
    #[default]
    Broad,
    Desktop,
    Input,
    Power,
}
impl Preset {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Broad => "broad",
            Self::Desktop => "desktop",
            Self::Input => "input",
            Self::Power => "power",
        }
    }
    fn includes_domain(self, name: &str) -> bool {
        match self {
            Self::Broad => true,
            Self::Desktop => matches!(
                name,
                "com.apple.dock" | "com.apple.finder" | "com.apple.screencapture"
            ),
            Self::Input => matches!(
                name,
                "NSGlobalDomain"
                    | "com.apple.AppleMultitouchTrackpad"
                    | "com.apple.driver.AppleBluetoothMultitouch.trackpad"
            ),
            Self::Power => false,
        }
    }
    fn includes_key(self, domain: &str, key: &str) -> bool {
        match (self, domain) {
            (Self::Broad, _) => true,
            (Self::Desktop, "com.apple.dock") => matches!(
                key,
                "autohide"
                    | "orientation"
                    | "tilesize"
                    | "magnification"
                    | "largesize"
                    | "mineffect"
                    | "show-recents"
                    | "minimize-to-application"
                    | "mru-spaces"
            ),
            (Self::Desktop, "com.apple.finder") => matches!(
                key,
                "AppleShowAllFiles" | "ShowPathbar" | "ShowStatusBar" | "FXPreferredViewStyle"
            ),
            (Self::Desktop, "com.apple.screencapture") => {
                matches!(key, "type" | "show-thumbnail" | "showsClicks")
            }
            (Self::Input, "NSGlobalDomain") => matches!(
                key,
                "InitialKeyRepeat"
                    | "KeyRepeat"
                    | "AppleKeyboardUIMode"
                    | "com.apple.swipescrolldirection"
            ),
            (
                Self::Input,
                "com.apple.AppleMultitouchTrackpad"
                | "com.apple.driver.AppleBluetoothMultitouch.trackpad",
            ) => matches!(
                key,
                "Clicking" | "TrackpadRightClick" | "TrackpadThreeFingerDrag"
            ),
            _ => false,
        }
    }
}
impl fmt::Display for Preset {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}
impl FromStr for Preset {
    type Err = Error;
    fn from_str(value: &str) -> Result<Self> {
        match value {
            "broad" => Ok(Self::Broad),
            "desktop" => Ok(Self::Desktop),
            "input" => Ok(Self::Input),
            "power" => Ok(Self::Power),
            _ => Err(Error::Invalid(format!(
                "unknown preset {value:?}; expected broad, desktop, input, or power"
            ))),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct CaptureReport {
    /// Captured settings only; failed reads never become preference nodes.
    pub document: Document,
    /// Capture bookkeeping for callers, never serialized into the document.
    pub omissions: Vec<Unavailable>,
    pub diagnostics: Vec<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EntryKind {
    File,
    Directory,
    Symlink,
    Other,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CaptureEntry {
    /// A single filename, never an absolute path or a relative traversal.
    pub name: OsString,
    pub kind: EntryKind,
}

/// Logical paths use the offline layout, independent of the adapter's storage.
/// Implementations must not substitute live state for unavailable inputs.
/// The two named provider reads deliberately do not expose a command runner.
pub trait CaptureReader {
    fn read_dir(&self, path: &Path) -> io::Result<Vec<CaptureEntry>>;
    fn read_file(&self, path: &Path) -> io::Result<Vec<u8>>;
    fn host_id(&self) -> io::Result<String>;
    fn power_text(&self) -> io::Result<String>;
}

pub struct OfflineReader {
    root: PathBuf,
}
impl OfflineReader {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }
    fn resolve(&self, path: &Path) -> io::Result<PathBuf> {
        check_relative(path)?;
        Ok(self.root.join(path))
    }
}
impl CaptureReader for OfflineReader {
    fn read_dir(&self, path: &Path) -> io::Result<Vec<CaptureEntry>> {
        read_directory(&self.resolve(path)?)
    }
    fn read_file(&self, path: &Path) -> io::Result<Vec<u8>> {
        read_bytes(&self.resolve(path)?)
    }
    fn host_id(&self) -> io::Result<String> {
        read_text(self.read_file(Path::new("host-id.txt"))?)
    }
    fn power_text(&self) -> io::Result<String> {
        read_text(self.read_file(Path::new("pmset.txt"))?)
    }
}

pub fn capture_offline(root: &Path, preset: Preset) -> Result<CaptureReport> {
    capture_with_reader(&OfflineReader::new(root), preset)
}

pub fn capture_live(preset: Preset) -> Result<CaptureReport> {
    #[cfg(target_os = "macos")]
    {
        capture_with_reader(&native::NativeReader::new()?, preset)
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = preset;
        Err(Error::Invalid(
            "live capture requires macOS; use --from for an offline input tree".into(),
        ))
    }
}

/// Capture only through the supplied read boundary; useful for denied reads and
/// source enumeration tests without touching the machine's preference stores.
pub fn capture_with_reader(reader: &impl CaptureReader, preset: Preset) -> Result<CaptureReport> {
    let mut capture = Capture {
        reader,
        preset,
        document: Document::default(),
        omissions: Vec::new(),
        diagnostics: Vec::new(),
        host: None,
        host_attempted: false,
        sources: BTreeMap::new(),
        filtered_keys: 0,
        excluded_sources: 0,
    };
    match preset {
        Preset::Broad => {
            capture.resolve_host();
            capture.preferences(Path::new("user"), Scope::User, None, false);
            capture.preferences(Path::new("system"), Scope::System, None, false);
            capture.containers();
            capture.group_containers();
            capture.power();
            for provider in UNSUPPORTED_PROVIDERS {
                capture.unavailable(
                    format!("provider:{provider}"),
                    Reason::Unsupported,
                    "dedicated provider is outside persisted defaults and pmset capture",
                );
            }
        }
        Preset::Desktop | Preset::Input => {
            capture.preferences(Path::new("user"), Scope::User, None, false)
        }
        Preset::Power => capture.power(),
    }
    capture.document.validate()?;
    capture.omissions.sort();
    capture.omissions.dedup();
    let included = capture
        .document
        .domains
        .values()
        .chain(capture.document.power.values())
        .map(BTreeMap::len)
        .sum::<usize>();
    let exclusions = match preset {
        Preset::Broad => {
            "none by preset; unsupported providers and unavailable sources are listed separately"
        }
        Preset::Desktop => {
            "non-desktop keys/domains, system and container stores, group containers, power and dedicated providers"
        }
        Preset::Input => {
            "non-input keys/domains, system and container stores, group containers, power and dedicated providers"
        }
        Preset::Power => {
            "all preference stores, group containers and non-power dedicated providers"
        }
    };
    capture.diagnostics.push(format!("preset {preset}: included {included} keys; filtered {} keys in selected domains; intentionally excluded {} discovered sources; exclusions: {exclusions}", capture.filtered_keys, capture.excluded_sources));
    Ok(CaptureReport {
        document: capture.document,
        omissions: capture.omissions,
        diagnostics: capture.diagnostics,
    })
}

const UNSUPPORTED_PROVIDERS: &[&str] = &[
    "systemsetup",
    "network",
    "tcc",
    "managed-preferences",
    "nvram",
    "boot",
    "accounts-security",
    "sharing",
    "firewall",
    "printing",
    "time-machine",
    "wallpaper",
    "displays",
    "spaces",
    "menus",
    "accessibility-databases",
    "login-background-items",
    "launch-services",
    "browser-databases",
    "registered-defaults",
    "unsaved-caches",
];

struct Capture<'a, R> {
    reader: &'a R,
    preset: Preset,
    document: Document,
    omissions: Vec<Unavailable>,
    diagnostics: Vec<String>,
    host: Option<String>,
    host_attempted: bool,
    sources: BTreeMap<Domain, String>,
    filtered_keys: usize,
    excluded_sources: usize,
}
impl<R: CaptureReader> Capture<'_, R> {
    fn unavailable(&mut self, source: String, reason: Reason, detail: impl fmt::Display) {
        self.diagnostics
            .push(format!("{source}: {}: {detail}", reason.as_str()));
        self.omissions.push(Unavailable { source, reason });
    }
    fn read_error(&mut self, source: String, error: io::Error) {
        let reason = match error.kind() {
            io::ErrorKind::NotFound => Reason::Missing,
            io::ErrorKind::PermissionDenied => Reason::PermissionDenied,
            io::ErrorKind::Unsupported => Reason::Unsupported,
            _ => Reason::ReadFailed,
        };
        self.unavailable(source, reason, error);
    }
    fn list(&mut self, path: &Path, source: String) -> Option<Vec<CaptureEntry>> {
        match self.reader.read_dir(path) {
            Ok(mut entries) => {
                entries.sort_by(|a, b| a.name.cmp(&b.name));
                Some(entries)
            }
            Err(error) => {
                self.read_error(source, error);
                None
            }
        }
    }
    fn entry_path(&mut self, parent: &Path, entry: &CaptureEntry) -> Option<PathBuf> {
        if !matches!(
            Path::new(&entry.name).components().next(),
            Some(Component::Normal(_))
        ) || Path::new(&entry.name).components().count() != 1
        {
            self.unavailable(
                format!("defaults:{}", parent.display()),
                Reason::ReadFailed,
                "reader returned a filename containing a path traversal",
            );
            None
        } else {
            Some(parent.join(&entry.name))
        }
    }
    fn resolve_host(&mut self) {
        if self.host_attempted {
            return;
        }
        self.host_attempted = true;
        match self.reader.host_id() {
            Ok(raw) if valid_uuid(raw.trim()) => self.host = Some(raw.trim().to_owned()),
            Ok(_) => self.unavailable(
                "provider:host-id".into(),
                Reason::ReadFailed,
                "malformed source UUID; expected one complete UUID",
            ),
            Err(error) => self.read_error("provider:host-id".into(), error),
        }
    }
    fn preferences(&mut self, path: &Path, scope: Scope, container: Option<&str>, by_host: bool) {
        let Some(entries) = self.list(path, format!("defaults:{}", path.display())) else {
            return;
        };
        for entry in entries {
            let Some(file_path) = self.entry_path(path, &entry) else {
                continue;
            };
            let source = format!("defaults:{}", file_path.display());
            let Some(name) = entry.name.to_str() else {
                self.unavailable(
                    source,
                    Reason::Unsupported,
                    "non-Unicode preference filename has no domain mapping",
                );
                continue;
            };
            if entry.kind == EntryKind::Directory && name == "ByHost" && !by_host {
                self.preferences(&file_path, scope, container, true);
                continue;
            }
            if entry.kind == EntryKind::Symlink && name == "ByHost" {
                self.unavailable(source, Reason::Unsupported, "ByHost symlink refused");
                continue;
            }
            if entry.kind == EntryKind::Directory {
                if self.preset != Preset::Broad {
                    self.excluded_sources += 1;
                } else {
                    self.unavailable(
                        source,
                        Reason::Unsupported,
                        "nested/special preference directory has no approved scope mapping",
                    );
                }
                continue;
            }
            let Some(stem) = name.strip_suffix(".plist") else {
                if self.preset == Preset::Broad {
                    self.unavailable(
                        source,
                        Reason::Unsupported,
                        "source is not a plist preference file",
                    );
                } else {
                    self.excluded_sources += 1;
                }
                continue;
            };
            let (raw_domain, suffix) = if by_host {
                match stem.rsplit_once('.') {
                    Some((domain, suffix)) => (domain, Some(suffix)),
                    None => {
                        self.unavailable(
                            source,
                            Reason::Unsupported,
                            "ByHost filename has no source UUID suffix",
                        );
                        continue;
                    }
                }
            } else {
                (stem, None)
            };
            let name = if raw_domain == ".GlobalPreferences" {
                "NSGlobalDomain"
            } else {
                raw_domain
            };
            if !self.preset.includes_domain(name) {
                self.excluded_sources += 1;
                continue;
            }
            if entry.kind != EntryKind::File {
                let detail = if entry.kind == EntryKind::Symlink {
                    "symlink refused"
                } else {
                    "non-regular preference source refused"
                };
                self.unavailable(source, Reason::Unsupported, detail);
                continue;
            }
            if name.is_empty()
                || name.split('.').any(str::is_empty)
                || name.split('.').count() > MAX_DEPTH
            {
                self.unavailable(
                    source,
                    Reason::Unsupported,
                    "filename has no valid domain namespace",
                );
                continue;
            }
            if scope == Scope::System && specialized_system_domain(name) {
                self.unavailable(
                    source,
                    Reason::Unsupported,
                    "special system owner requires its dedicated power/network mapping",
                );
                continue;
            }
            if let Some(suffix) = suffix {
                self.resolve_host();
                if !valid_uuid(suffix) {
                    self.unavailable(
                        source,
                        Reason::Unsupported,
                        "ByHost filename has a malformed source UUID suffix",
                    );
                    continue;
                }
                match &self.host {
                    Some(host) if host.eq_ignore_ascii_case(suffix) => (),
                    Some(_) => {
                        self.unavailable(
                            source,
                            Reason::Unsupported,
                            "foreign source host; not current-host data",
                        );
                        continue;
                    }
                    None => {
                        self.unavailable(
                            source,
                            Reason::Unsupported,
                            "current source host is unresolved; no host is assumed current",
                        );
                        continue;
                    }
                }
            }
            let domain = Domain {
                name: name.to_owned(),
                scope,
                host: if by_host { Host::Current } else { Host::Any },
                container: container.map(str::to_owned),
            };
            if let Some(previous) = self.sources.get(&domain).cloned() {
                self.document.domains.remove(&domain);
                self.unavailable(
                    previous.clone(),
                    Reason::ReadFailed,
                    format!("duplicate scoped identity also supplied by {source}"),
                );
                self.unavailable(
                    source,
                    Reason::ReadFailed,
                    format!("duplicate scoped identity also supplied by {previous}"),
                );
                continue;
            }
            self.sources.insert(domain.clone(), source.clone());
            let bytes = match self.reader.read_file(&file_path) {
                Ok(bytes) => bytes,
                Err(error) => {
                    self.read_error(source, error);
                    continue;
                }
            };
            let mut properties = match parse_plist(&bytes) {
                Ok(Value::Dict(properties)) => properties,
                Ok(_) => {
                    self.unavailable(
                        source,
                        Reason::ReadFailed,
                        "malformed preferences: plist root must be a dictionary",
                    );
                    continue;
                }
                Err(error) => {
                    self.unavailable(
                        source,
                        Reason::ReadFailed,
                        format!("malformed plist: {error}"),
                    );
                    continue;
                }
            };
            if properties.is_empty() {
                self.diagnostics.push(format!(
                    "{source}: empty preference dictionary; no stored keys"
                ));
                continue;
            }
            let before = properties.len();
            properties.retain(|key, _| self.preset.includes_key(name, key));
            self.filtered_keys += before - properties.len();
            if !properties.is_empty() {
                self.document.domains.insert(domain, properties);
            }
        }
    }
    fn containers(&mut self) {
        let path = Path::new("containers");
        let Some(entries) = self.list(path, "defaults:containers".into()) else {
            return;
        };
        for entry in entries {
            let Some(container_path) = self.entry_path(path, &entry) else {
                continue;
            };
            let source = format!("defaults:{}", container_path.display());
            match (entry.name.to_str(), entry.kind) {
                (Some(id), EntryKind::Directory) => {
                    self.preferences(&container_path, Scope::User, Some(id), false)
                }
                (_, EntryKind::Symlink) => {
                    self.unavailable(source, Reason::Unsupported, "container symlink refused")
                }
                _ => self.unavailable(
                    source,
                    Reason::Unsupported,
                    "container has no representable directory identity",
                ),
            }
        }
    }
    fn group_containers(&mut self) {
        let path = Path::new("group-containers");
        let Some(entries) = self.list(path, "provider:group-containers".into()) else {
            return;
        };
        if entries.is_empty() {
            self.diagnostics
                .push("provider:group-containers: no group containers discovered".into());
        }
        for entry in entries {
            let Some(path) = self.entry_path(path, &entry) else {
                continue;
            };
            self.unavailable(format!("provider:{}", path.display()), Reason::Unsupported,
                "group-container store has no selector in the approved schema; contents were not read");
        }
    }
    fn power(&mut self) {
        match self.reader.power_text() {
            Ok(text) => self.parse_power(&text),
            Err(error) => self.read_error("provider:pmset".into(), error),
        }
    }
    fn parse_power(&mut self, text: &str) {
        if text.len() > MAX_DOCUMENT_BYTES {
            self.unavailable(
                "provider:pmset".into(),
                Reason::ReadFailed,
                "power input exceeds size limit",
            );
            return;
        }
        let mut current: Option<String> = None;
        let mut headings = BTreeSet::new();
        let mut invalid = BTreeSet::new();
        let mut keys: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
        let mut profiles: BTreeMap<String, Properties> = BTreeMap::new();
        for (index, raw_line) in text.lines().enumerate() {
            let line = raw_line.trim();
            if line.is_empty() {
                continue;
            }
            if let Some(heading) = line
                .strip_suffix(':')
                .filter(|_| !raw_line.starts_with(char::is_whitespace))
            {
                if heading.is_empty() {
                    self.unavailable(
                        format!("provider:pmset:line:{}", index + 1),
                        Reason::ReadFailed,
                        "empty power heading",
                    );
                    current = None;
                    continue;
                }
                let profile = match heading {
                    "Battery Power" => "battery".to_owned(),
                    "AC Power" => "charger".to_owned(),
                    "UPS Power" => "ups".to_owned(),
                    _ => format!("unknown:{heading}"),
                };
                if !headings.insert(profile.clone()) {
                    invalid.insert(profile.clone());
                    self.unavailable(
                        format!("pmset:{profile}"),
                        Reason::ReadFailed,
                        "duplicate or ambiguous power profile heading",
                    );
                } else if !matches!(profile.as_str(), "battery" | "charger" | "ups") {
                    self.unavailable(
                        format!("pmset:{profile}"),
                        Reason::Unsupported,
                        "unknown power profile; no all-profiles mapping is inferred",
                    );
                }
                keys.entry(profile.clone()).or_default();
                current = Some(profile);
                continue;
            }
            let Some(profile) = current.as_ref() else {
                self.unavailable(
                    format!("provider:pmset:line:{}", index + 1),
                    Reason::ReadFailed,
                    "power setting precedes a recognized heading boundary",
                );
                continue;
            };
            let parts: Vec<_> = line.split_whitespace().collect();
            // Native output can contain display labels such as "Sleep On Power
            // Button". Account for those unsupported fields without discarding
            // the valid settings beside them. Extra tokens on a known command
            // key are still malformed, not an invented display label.
            if parts.len() < 2
                || !parts[..parts.len() - 1]
                    .iter()
                    .all(|part| part.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'_'))
                || (parts.len() != 2 && power_key_supported(profile, parts[0]))
            {
                invalid.insert(profile.clone());
                self.unavailable(
                    format!("pmset:{profile}:line:{}", index + 1),
                    Reason::ReadFailed,
                    "malformed power key/value line",
                );
                continue;
            }
            let key = parts[..parts.len() - 1].join(" ");
            if !keys
                .entry(profile.clone())
                .or_default()
                .insert(key.to_owned())
            {
                invalid.insert(profile.clone());
                self.unavailable(
                    format!("pmset:{profile}/{key}"),
                    Reason::ReadFailed,
                    "duplicate power key",
                );
                continue;
            }
            if !power_key_supported(profile, &key) {
                self.unavailable(
                    format!("pmset:{profile}/{key}"),
                    Reason::Unsupported,
                    "unknown profile or non-writable/unknown power key",
                );
                continue;
            }
            let Ok(value) = parts[parts.len() - 1].parse::<i64>() else {
                invalid.insert(profile.clone());
                self.unavailable(
                    format!("pmset:{profile}/{key}"),
                    Reason::ReadFailed,
                    "malformed native integer value",
                );
                continue;
            };
            profiles
                .entry(profile.clone())
                .or_default()
                .insert(key.to_owned(), Value::Integer(value.into()).into());
        }
        if headings.is_empty() {
            self.unavailable(
                "provider:pmset".into(),
                Reason::ReadFailed,
                "malformed power input: no profile headings",
            );
        }
        for (profile, seen) in &keys {
            if seen.is_empty() && matches!(profile.as_str(), "battery" | "charger" | "ups") {
                invalid.insert(profile.clone());
                self.unavailable(
                    format!("pmset:{profile}"),
                    Reason::ReadFailed,
                    "empty power profile",
                );
            }
        }
        for profile in invalid {
            profiles.remove(&profile);
            self.unavailable(
                format!("pmset:{profile}"),
                Reason::ReadFailed,
                "rejected the complete profile because its data is ambiguous or malformed",
            );
        }
        self.document.power = profiles;
    }
}

fn specialized_system_domain(name: &str) -> bool {
    name == "SystemConfiguration"
        || name == "com.apple.AutoWake"
        || name == "com.apple.PowerManagement"
        || name.starts_with("com.apple.PowerManagement.")
        || name == "com.apple.network"
        || name.starts_with("com.apple.network.")
}
fn valid_uuid(uuid: &str) -> bool {
    uuid.len() == 36
        && uuid.bytes().enumerate().all(|(i, c)| {
            if matches!(i, 8 | 13 | 18 | 23) {
                c == b'-'
            } else {
                c.is_ascii_hexdigit()
            }
        })
}

/// Build from the event stream so duplicate keys are rejected before a plist
/// dictionary could overwrite them, including duplicates inside nested values.
fn parse_plist(bytes: &[u8]) -> Result<Value> {
    if bytes.len() > MAX_DOCUMENT_BYTES {
        return Err(Error::Invalid("plist exceeds size limit".into()));
    }
    let mut events = PlistEvents {
        inner: plist::stream::Reader::new(Cursor::new(bytes)),
        remaining: MAX_DOCUMENT_BYTES / 8,
    };
    let first = events
        .next()?
        .ok_or_else(|| Error::Invalid("empty plist".into()))?;
    let value = parse_event(first, &mut events, 0)?;
    if events.next()?.is_some() {
        return Err(Error::Invalid("multiple plist root values".into()));
    }
    Ok(value)
}
struct PlistEvents<I> {
    inner: I,
    remaining: usize,
}
impl<I: Iterator<Item = std::result::Result<plist::stream::OwnedEvent, plist::Error>>>
    PlistEvents<I>
{
    fn next(&mut self) -> Result<Option<plist::stream::OwnedEvent>> {
        match self.inner.next() {
            None => Ok(None),
            Some(event) => {
                self.remaining = self
                    .remaining
                    .checked_sub(1)
                    .ok_or_else(|| Error::Invalid("plist exceeds expanded event limit".into()))?;
                event
                    .map(Some)
                    .map_err(|error| Error::Invalid(error.to_string()))
            }
        }
    }
}
fn parse_event<I>(
    event: plist::stream::OwnedEvent,
    events: &mut PlistEvents<I>,
    depth: usize,
) -> Result<Value>
where
    I: Iterator<Item = std::result::Result<plist::stream::OwnedEvent, plist::Error>>,
{
    use plist::stream::Event;
    if depth > MAX_DEPTH {
        return Err(Error::Invalid("plist exceeds nesting limit of 128".into()));
    }
    Ok(match event {
        Event::StartDictionary(_) => {
            let mut properties = Properties::new();
            loop {
                let event = events
                    .next()?
                    .ok_or_else(|| Error::Invalid("unclosed plist dictionary".into()))?;
                let key = match event {
                    Event::EndCollection => break,
                    Event::String(key) => key.into_owned(),
                    _ => {
                        return Err(Error::Invalid(
                            "plist dictionary key must be a string".into(),
                        ));
                    }
                };
                if properties.contains_key(&key) {
                    return Err(Error::Invalid(format!(
                        "duplicate plist dictionary key {key:?}"
                    )));
                }
                let event = events.next()?.ok_or_else(|| {
                    Error::Invalid(format!("missing value for plist key {key:?}"))
                })?;
                properties.insert(key, Setting::new(parse_event(event, events, depth + 1)?));
            }
            Value::Dict(properties)
        }
        Event::StartArray(_) => {
            let mut values = Vec::new();
            loop {
                let event = events
                    .next()?
                    .ok_or_else(|| Error::Invalid("unclosed plist array".into()))?;
                if matches!(event, Event::EndCollection) {
                    break;
                }
                values.push(Setting::new(parse_event(event, events, depth + 1)?));
            }
            Value::Array(values)
        }
        Event::Boolean(v) => Value::Boolean(v),
        Event::Data(v) => Value::Data(v.into_owned()),
        Event::Date(v) => Value::Date(v),
        Event::Real(v) => Value::Real(v),
        Event::String(v) => Value::String(v.into_owned()),
        Event::Uid(v) => Value::Uid(v.get()),
        Event::Integer(v) => Value::Integer(if let Some(n) = v.as_signed() {
            n.into()
        } else {
            v.as_unsigned()
                .ok_or_else(|| Error::Invalid("invalid plist integer".into()))?
                .into()
        }),
        _ => return Err(Error::Invalid("unexpected plist event".into())),
    })
}

fn check_relative(path: &Path) -> io::Result<()> {
    if path.as_os_str().is_empty()
        || path
            .components()
            .any(|c| !matches!(c, Component::Normal(_)))
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "expected a logical relative source path",
        ));
    }
    Ok(())
}
fn checked_metadata(path: &Path) -> io::Result<fs::Metadata> {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()?.join(path)
    };
    let mut prefix = PathBuf::new();
    for component in absolute.components() {
        prefix.push(component.as_os_str());
        if fs::symlink_metadata(&prefix)?.file_type().is_symlink() {
            return Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "symlink in source path refused",
            ));
        }
    }
    fs::symlink_metadata(absolute)
}
fn read_directory(path: &Path) -> io::Result<Vec<CaptureEntry>> {
    if !checked_metadata(path)?.is_dir() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "source directory is not a directory",
        ));
    }
    fs::read_dir(path)?
        .map(|entry| {
            let entry = entry?;
            let ty = entry.file_type()?;
            let kind = if ty.is_symlink() {
                EntryKind::Symlink
            } else if ty.is_dir() {
                EntryKind::Directory
            } else if ty.is_file() {
                EntryKind::File
            } else {
                EntryKind::Other
            };
            Ok(CaptureEntry {
                name: entry.file_name(),
                kind,
            })
        })
        .collect()
}
fn read_bytes(path: &Path) -> io::Result<Vec<u8>> {
    let metadata = checked_metadata(path)?;
    if !metadata.is_file() {
        return Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "non-regular source file refused",
        ));
    }
    if metadata.len() > MAX_DOCUMENT_BYTES as u64 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "source exceeds size limit",
        ));
    }
    let mut bytes = Vec::new();
    fs::File::open(path)?
        .take(MAX_DOCUMENT_BYTES as u64 + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() > MAX_DOCUMENT_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "source exceeds size limit",
        ));
    }
    Ok(bytes)
}
fn read_text(bytes: Vec<u8>) -> io::Result<String> {
    String::from_utf8(bytes)
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "source is not UTF-8 text"))
}

#[cfg(target_os = "macos")]
mod native {
    use super::*;
    use std::process::Command;

    pub(super) struct NativeReader {
        home: PathBuf,
    }
    impl NativeReader {
        pub(super) fn new() -> Result<Self> {
            let home = std::env::var_os("HOME").map(PathBuf::from).ok_or_else(|| {
                Error::Invalid("HOME is unavailable for native user preference discovery".into())
            })?;
            if !home.is_absolute() {
                return Err(Error::Invalid("HOME must be an absolute path".into()));
            }
            Ok(Self { home })
        }
        fn resolve(&self, path: &Path) -> io::Result<PathBuf> {
            check_relative(path)?;
            let mut components = path.components();
            let store = components
                .next()
                .expect("validated nonempty path")
                .as_os_str();
            let suffix = components.as_path();
            if store == "user" {
                return Ok(self.home.join("Library/Preferences").join(suffix));
            }
            if store == "system" {
                return Ok(Path::new("/Library/Preferences").join(suffix));
            }
            if store == "group-containers" {
                return Ok(self.home.join("Library/Group Containers").join(suffix));
            }
            if store == "containers" {
                let mut components = suffix.components();
                let Some(id) = components.next() else {
                    return Ok(self.home.join("Library/Containers"));
                };
                return Ok(self
                    .home
                    .join("Library/Containers")
                    .join(id.as_os_str())
                    .join("Data/Library/Preferences")
                    .join(components.as_path()));
            }
            Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "unknown native source store",
            ))
        }
    }
    impl CaptureReader for NativeReader {
        fn read_dir(&self, path: &Path) -> io::Result<Vec<CaptureEntry>> {
            read_directory(&self.resolve(path)?)
        }
        fn read_file(&self, path: &Path) -> io::Result<Vec<u8>> {
            read_bytes(&self.resolve(path)?)
        }
        fn host_id(&self) -> io::Result<String> {
            let output = Command::new("/usr/sbin/ioreg")
                .args(["-a", "-r", "-d", "1", "-c", "IOPlatformExpertDevice"])
                .output()?;
            if !output.status.success() {
                return Err(command_failure("ioreg", &output.stderr));
            }
            let value = parse_plist(&output.stdout)
                .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error.to_string()))?;
            let Value::Array(devices) = value else {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "ioreg did not return a device array",
                ));
            };
            let mut identifiers = Vec::new();
            for device in devices {
                if let Value::Dict(properties) = device.value
                    && let Some(setting) = properties.get("IOPlatformUUID")
                {
                    match &setting.value {
                        Value::String(uuid) if valid_uuid(uuid) => identifiers.push(uuid.clone()),
                        _ => {
                            return Err(io::Error::new(
                                io::ErrorKind::InvalidData,
                                "ioreg returned an invalid IOPlatformUUID",
                            ));
                        }
                    }
                }
            }
            if identifiers.len() != 1 {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "ioreg did not identify exactly one platform UUID",
                ));
            }
            Ok(identifiers.remove(0))
        }
        fn power_text(&self) -> io::Result<String> {
            let output = Command::new("/usr/bin/pmset")
                .args(["-g", "custom"])
                .output()?;
            if !output.status.success() {
                return Err(command_failure("pmset", &output.stderr));
            }
            read_text(output.stdout)
        }
    }
    fn command_failure(provider: &str, stderr: &[u8]) -> io::Error {
        let text = String::from_utf8_lossy(stderr);
        let lower = text.to_ascii_lowercase();
        let kind = if lower.contains("permission denied") || lower.contains("not permitted") {
            io::ErrorKind::PermissionDenied
        } else {
            io::ErrorKind::Other
        };
        io::Error::new(kind, format!("{provider} read failed: {}", text.trim()))
    }
}
