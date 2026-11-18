//! Pure support analysis shared by capture and native command generation.
use crate::model::power_key_supported;
use crate::{Document, Domain, Host, Scope, Value};
use std::collections::{BTreeMap, BTreeSet};
use std::time::{SystemTime, UNIX_EPOCH};

mod report;
pub use report::GenerationFailures;
use report::Issue;

#[derive(Default)]
pub(crate) struct Support {
    /// Rejected top-level keys, indexed by their complete scoped identity.
    pub defaults: BTreeMap<Domain, BTreeSet<String>>,
    pub power: BTreeMap<String, BTreeSet<String>>,
    pub failures: GenerationFailures,
}

impl Support {
    fn reject_default(&mut self, domain: &Domain, key: &str, issues: Vec<Issue>) {
        self.defaults
            .entry(domain.clone())
            .or_default()
            .insert(key.into());
        self.failures.push(issues, true);
    }
}

/// Analyze the complete selected domain: an XML-requiring sibling can constrain
/// otherwise representable string values. No host reads or commands occur here.
pub(crate) fn analyze(document: &Document) -> Support {
    let mut support = Support::default();
    for (domain, properties) in &document.domains {
        if let Some(reason) = domain_issue(domain) {
            if properties.is_empty() {
                support.failures.reject(&domain.identity(), reason, false);
            }
            support
                .defaults
                .insert(domain.clone(), properties.keys().cloned().collect());
            for key in properties.keys() {
                support.failures.reject(
                    &format!("{} key {key:?}", domain.identity()),
                    reason,
                    true,
                );
            }
            continue;
        }
        let mut retained = BTreeMap::new();
        for (key, setting) in properties {
            let identity = format!("{} key {key:?}", domain.identity());
            let mut issues = Vec::new();
            if let Err(error) = setting.value.validate(domain.name.split('.').count() + 1) {
                issues.push(Issue::with_detail(
                    "value cannot be represented in KDL",
                    format!("{identity}: {error}"),
                ));
            } else {
                if key.starts_with('-') {
                    issues.push(Issue::new(
                        &identity,
                        "top-level key starts with an option marker",
                    ));
                }
                check_text(key, &identity, false, &mut issues);
                check_value(&setting.value, &identity, &mut issues);
                if requires_xml(&setting.value) {
                    check_xml(&setting.value, &identity, &mut issues);
                    check_text(key, &identity, true, &mut issues);
                }
            }
            if issues.is_empty() {
                retained.insert(key.as_str(), setting);
            } else {
                support.reject_default(domain, key, issues);
            }
        }
        if retained
            .values()
            .any(|setting| requires_xml(&setting.value))
        {
            for (key, setting) in retained {
                let identity = format!("{} key {key:?}", domain.identity());
                let mut issues = Vec::new();
                check_text(key, &identity, true, &mut issues);
                check_xml(&setting.value, &identity, &mut issues);
                if !issues.is_empty() {
                    support.reject_default(domain, key, issues);
                }
            }
        }
    }
    for (profile, properties) in &document.power {
        let known = matches!(profile.as_str(), "battery" | "charger" | "ups");
        if !known && properties.is_empty() {
            support.power.entry(profile.clone()).or_default();
            support.failures.reject(
                &format!("pmset profile {profile:?}"),
                "unsupported power profile",
                false,
            );
        }
        for (key, setting) in properties {
            let identity = format!("pmset profile {profile:?} key {key:?}");
            let issue = if !known {
                Some(Issue::new(&identity, "unsupported power profile"))
            } else if !power_key_supported(profile, key) {
                Some(Issue::new(
                    &identity,
                    "key has no documented writable pmset mapping for this profile",
                ))
            } else {
                power_value_issue(key, &setting.value, &identity)
            };
            if let Some(issue) = issue {
                support
                    .power
                    .entry(profile.clone())
                    .or_default()
                    .insert(key.clone());
                support.failures.push(vec![issue], true);
            }
        }
    }
    support.failures.diagnostics.sort();
    support
}

pub(crate) fn domain_issue(domain: &Domain) -> Option<&'static str> {
    if !safe_identifier(&domain.name) {
        return Some(
            "unsafe domain identity (use nonempty alphanumeric, underscore, or hyphen segments)",
        );
    }
    if let Some(container) = &domain.container
        && !safe_identifier(container)
    {
        return Some("unsafe container identity (paths and option markers are not supported)");
    }
    if domain.scope == Scope::System && domain.container.is_some() {
        return Some("system container scope has no supported native mapping");
    }
    if domain.host == Host::Current && (domain.scope == Scope::System || domain.container.is_some())
    {
        return Some("system or container current-host scope has no supported native mapping");
    }
    None
}

fn safe_identifier(value: &str) -> bool {
    !value.is_empty()
        && !value.starts_with('-')
        && value.split('.').all(|part| {
            !part.is_empty()
                && part
                    .chars()
                    .all(|ch| ch.is_alphanumeric() || matches!(ch, '_' | '-'))
        })
}

fn check_value(value: &Value, path: &str, issues: &mut Vec<Issue>) {
    match value {
        Value::String(text) => check_text(text, path, false, issues),
        Value::Uid(_) => issues.push(Issue::new(
            path,
            "UID has no supported native representation",
        )),
        Value::Integer(value) if *value > i64::MAX as i128 => {
            issues.push(Issue::new(
                path,
                "integer exceeds signed 64-bit native range",
            ));
        }
        Value::Real(value) if !value.is_finite() => {
            issues.push(Issue::new(
                path,
                "nonfinite real has no supported native representation",
            ));
        }
        Value::Date(date) => {
            let time = SystemTime::from(*date);
            let (seconds, nanos) = match time.duration_since(UNIX_EPOCH) {
                Ok(duration) => (duration.as_secs() as i128, duration.subsec_nanos()),
                Err(error) => (
                    -(error.duration().as_secs() as i128),
                    error.duration().subsec_nanos(),
                ),
            };
            if nanos != 0 {
                issues.push(Issue::new(
                    path,
                    "fractional-second date has no verified faithful native mapping",
                ));
            }
            // RFC 3339 formatting in plist is limited to four-digit nonnegative years.
            // Check SystemTime first: Date's public SystemTime constructor is unchecked.
            if !(-62_167_219_200..=253_402_300_799).contains(&seconds) {
                issues.push(Issue::new(
                    path,
                    "date is outside the supported four-digit year range",
                ));
            }
        }
        Value::Array(values) => {
            for (index, setting) in values.iter().enumerate() {
                check_value(&setting.value, &format!("{path}[{index}]"), issues);
            }
        }
        Value::Dict(values) => {
            for (key, setting) in values {
                let path = format!("{path}[{key:?}]");
                check_text(key, &path, false, issues);
                check_value(&setting.value, &path, issues);
            }
        }
        _ => {}
    }
}

fn check_text(text: &str, path: &str, xml: bool, issues: &mut Vec<Issue>) {
    if text.contains('\0') {
        issues.push(Issue::new(path, "embedded NUL is unsupported"));
    }
    if xml
        && text.chars().any(|ch| {
            !(matches!(ch, '\t' | '\n' | '\r')
                || (ch >= ' ' && !matches!(ch, '\u{fffe}' | '\u{ffff}')))
        })
    {
        issues.push(Issue::new(
            path,
            "text contains characters forbidden in the required XML plist",
        ));
    }
}

fn check_xml(value: &Value, path: &str, issues: &mut Vec<Issue>) {
    match value {
        Value::String(text) => check_text(text, path, true, issues),
        Value::Array(values) => {
            for (index, setting) in values.iter().enumerate() {
                check_xml(&setting.value, &format!("{path}[{index}]"), issues);
            }
        }
        Value::Dict(values) => {
            for (key, setting) in values {
                let path = format!("{path}[{key:?}]");
                check_text(key, &path, true, issues);
                check_xml(&setting.value, &path, issues);
            }
        }
        _ => {}
    }
}

pub(crate) fn requires_xml(value: &Value) -> bool {
    !value.is_scalar() && !openstep_supported(value)
}

fn openstep_supported(value: &Value) -> bool {
    match value {
        Value::String(_) | Value::Data(_) => true,
        Value::Array(values) => values
            .iter()
            .all(|setting| openstep_supported(&setting.value)),
        Value::Dict(values) => values
            .values()
            .all(|setting| openstep_supported(&setting.value)),
        _ => false,
    }
}

fn power_value_issue(key: &str, value: &Value, path: &str) -> Option<Issue> {
    let Value::Integer(value) = value else {
        return Some(Issue::new(
            path,
            "pmset requires an integer; no type coercion is performed",
        ));
    };
    let range = match key {
        "womp"
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
        | "ttyskeepawake"
        | "tcpkeepalive"
        | "destroyfvkeyonstandby"
        | "standby"
        | "autopoweroff" => {
            if matches!(*value, 0 | 1) {
                return None;
            }
            "0 or 1"
        }
        "hibernatemode" => {
            if matches!(*value, 0 | 3 | 25) {
                return None;
            }
            "0, 3, or 25"
        }
        "highstandbythreshold" => {
            if (0..=100).contains(value) {
                return None;
            }
            "0 through 100 percent"
        }
        "haltlevel" => {
            if (-1..=100).contains(value) {
                return None;
            }
            "-1 (disabled) or 0 through 100 percent"
        }
        "haltafter" | "haltremain" => {
            if (-1..=i32::MAX as i128).contains(value) {
                return None;
            }
            "-1 (disabled) or 0 through 2147483647 minutes"
        }
        "displaysleep" | "disksleep" | "sleep" | "standbydelayhigh" | "standbydelaylow"
        | "autopoweroffdelay" => {
            if (0..=i32::MAX as i128).contains(value) {
                return None;
            }
            "0 through 2147483647"
        }
        _ => return Some(Issue::new(path, "key has no verified value-range mapping")),
    };
    Some(Issue::with_detail(
        "unsupported pmset value",
        format!("{path}: unsupported pmset value {value}; expected {range}"),
    ))
}
