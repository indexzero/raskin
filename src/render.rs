//! Pure generation of inspectable native commands. This module never runs them.
use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::model::power_key_supported;
use crate::{Document, Domain, Error, Host, Result, Scope, Setting, Value};

#[derive(Clone, Copy, Debug, Default)]
pub struct GenerateOptions {
    /// Omit complete unsupported top-level settings and name each omission.
    pub skip_unsupported: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Generated {
    pub script: String,
    pub diagnostics: Vec<String>,
}

/// Validate and render saved data without consulting or changing any host.
pub fn generate(document: &Document, options: GenerateOptions) -> Result<Generated> {
    document.validate()?;
    let mut body = String::new();
    let mut skipped = Vec::new();
    let mut delimiter_number = 1;
    for (domain, properties) in &document.domains {
        let target = match defaults_target(domain) {
            Ok(target) => target,
            Err(reason) => {
                if properties.is_empty() {
                    skipped.push(format!("{}: {reason}", domain.identity()));
                }
                for key in properties.keys() {
                    skipped.push(format!("{} key {key:?}: {reason}", domain.identity()));
                }
                continue;
            }
        };
        let mut retained = BTreeMap::new();
        for (key, setting) in properties {
            let identity = format!("{} key {key:?}", domain.identity());
            let mut issues = Vec::new();
            if key.starts_with('-') {
                issues.push("top-level key starts with an option marker".into());
            }
            check_text(key, &identity, false, &mut issues);
            check_value(&setting.value, &identity, &mut issues);
            if requires_xml(&setting.value) {
                check_xml(&setting.value, &identity, &mut issues);
                check_text(key, &identity, true, &mut issues);
            }
            if issues.is_empty() {
                retained.insert(key.as_str(), setting);
            } else {
                skipped.push(format!("{identity}: {}", issues.join("; ")));
            }
        }
        let xml = retained
            .values()
            .any(|setting| requires_xml(&setting.value));
        if xml {
            retained.retain(|key, setting| {
                let identity = format!("{} key {key:?}", domain.identity());
                let mut issues = Vec::new();
                check_text(key, &identity, true, &mut issues);
                check_xml(&setting.value, &identity, &mut issues);
                if issues.is_empty() {
                    true
                } else {
                    skipped.push(format!("{identity}: {}", issues.join("; ")));
                    false
                }
            });
        }
        if retained.is_empty() {
            continue;
        }
        let _ = writeln!(body, "\n# {}", comment_text(&domain.identity()));
        if xml {
            let payload = xml_dictionary(&retained)?;
            let delimiter = loop {
                let candidate = format!("RASKIN_PLIST_{delimiter_number}");
                delimiter_number += 1;
                if !payload.contains(&candidate) {
                    break candidate;
                }
            };
            body.push_str(
                "# Native defaults import reads the emitted domain dictionary from stdin.\n",
            );
            body.push_str("# Its behavior for destination keys absent from this dictionary is not specified here.\n");
            let _ = writeln!(
                body,
                "{} import {} - <<'{delimiter}'",
                target.command, target.domain
            );
            body.push_str(&payload);
            if !payload.ends_with('\n') {
                body.push('\n');
            }
            let _ = writeln!(body, "{delimiter}");
        } else {
            for (key, setting) in retained {
                let _ = writeln!(
                    body,
                    "{} write {} {} {}",
                    target.command,
                    target.domain,
                    shell_quote(key),
                    defaults_value(&setting.value)
                );
            }
        }
    }
    for (profile, properties) in &document.power {
        let flag = match profile.as_str() {
            "battery" => Some("-b"),
            "charger" => Some("-c"),
            "ups" => Some("-u"),
            _ => None,
        };
        if flag.is_none() && properties.is_empty() {
            skipped.push(format!(
                "pmset profile {profile:?}: unsupported power profile"
            ));
        }
        let mut commands = String::new();
        for (key, setting) in properties {
            let identity = format!("pmset profile {profile:?} key {key:?}");
            let reason = if flag.is_none() {
                Some("unsupported power profile".into())
            } else if !power_key_supported(profile, key) {
                Some("key has no documented writable pmset mapping for this profile".into())
            } else {
                power_value_issue(key, &setting.value)
            };
            if let Some(reason) = reason {
                skipped.push(format!("{identity}: {reason}"));
                continue;
            }
            let Value::Integer(value) = setting.value else {
                unreachable!("power value checked above")
            };
            let _ = writeln!(
                commands,
                "sudo /usr/bin/pmset {} {} {}",
                flag.expect("power profile checked above"),
                shell_quote(key),
                shell_quote(&value.to_string())
            );
        }
        if !commands.is_empty() {
            let _ = writeln!(body, "\n# pmset: {}", comment_text(profile));
            body.push_str(&commands);
        }
    }
    skipped.sort();
    if !skipped.is_empty() && !options.skip_unsupported {
        return Err(Error::Unsupported(skipped.join("\n")));
    }
    let mut script = String::from(
        "#!/bin/sh\n# Generated by Raskin. Inspect before running on the destination Mac.\n# Native commands below define their own effects.\n",
    );
    let mut diagnostics = Vec::new();
    let mut unavailable = document.unavailable.iter().collect::<Vec<_>>();
    unavailable.sort();
    for item in unavailable {
        let diagnostic = format!(
            "capture unavailable: {:?} ({})",
            item.source,
            item.reason.as_str()
        );
        let _ = writeln!(script, "# {}", comment_text(&diagnostic));
        diagnostics.push(diagnostic);
    }
    if !skipped.is_empty() {
        let _ = writeln!(
            script,
            "# Partial output: {} unsupported items skipped.",
            skipped.len()
        );
        for diagnostic in skipped {
            let diagnostic = format!("Skipped: {diagnostic}");
            let _ = writeln!(script, "# {}", comment_text(&diagnostic));
            diagnostics.push(diagnostic);
        }
    }
    script.push_str(&body);
    Ok(Generated {
        script,
        diagnostics,
    })
}

struct DefaultsTarget {
    command: String,
    domain: String,
}

fn defaults_target(domain: &Domain) -> std::result::Result<DefaultsTarget, &'static str> {
    if !safe_identifier(&domain.name) {
        return Err(
            "unsafe domain identity (use nonempty alphanumeric, underscore, or hyphen segments)",
        );
    }
    if let Some(container) = &domain.container
        && !safe_identifier(container)
    {
        return Err("unsafe container identity (paths and option markers are not supported)");
    }
    if domain.scope == Scope::System && domain.container.is_some() {
        return Err("system container scope has no supported native mapping");
    }
    if domain.host == Host::Current && (domain.scope == Scope::System || domain.container.is_some())
    {
        return Err("system or container current-host scope has no supported native mapping");
    }
    let command = match (domain.scope, domain.host) {
        (Scope::System, _) => "sudo /usr/bin/defaults",
        (Scope::User, Host::Current) => "/usr/bin/defaults -currentHost",
        (Scope::User, Host::Any) => "/usr/bin/defaults",
    }
    .to_owned();
    let filename = if domain.name == "NSGlobalDomain" {
        ".GlobalPreferences"
    } else {
        &domain.name
    };
    let target = match (&domain.container, domain.scope) {
        (Some(container), _) => format!(
            "\"$HOME\"{}",
            shell_quote(&format!(
                "/Library/Containers/{container}/Data/Library/Preferences/{filename}"
            ))
        ),
        (None, Scope::System) => shell_quote(&format!("/Library/Preferences/{filename}")),
        (None, Scope::User) => shell_quote(&domain.name),
    };
    Ok(DefaultsTarget {
        command,
        domain: target,
    })
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

fn check_value(value: &Value, path: &str, issues: &mut Vec<String>) {
    match value {
        Value::String(text) => check_text(text, path, false, issues),
        Value::Uid(_) => issues.push(format!(
            "{path}: UID has no supported native representation"
        )),
        Value::Integer(value) if *value > i64::MAX as i128 => {
            issues.push(format!(
                "{path}: integer exceeds signed 64-bit native range"
            ));
        }
        Value::Real(value) if !value.is_finite() => {
            issues.push(format!(
                "{path}: nonfinite real has no supported native representation"
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
                issues.push(format!(
                    "{path}: fractional-second date has no verified faithful native mapping"
                ));
            }
            // RFC 3339 formatting in plist is limited to four-digit nonnegative years.
            // Check SystemTime first: Date's public SystemTime constructor is unchecked.
            if !(-62_167_219_200..=253_402_300_799).contains(&seconds) {
                issues.push(format!(
                    "{path}: date is outside the supported four-digit year range"
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

fn check_text(text: &str, path: &str, xml: bool, issues: &mut Vec<String>) {
    if text.contains('\0') {
        issues.push(format!("{path}: embedded NUL is unsupported"));
    }
    if xml
        && text.chars().any(|ch| {
            !(matches!(ch, '\t' | '\n' | '\r')
                || (ch >= ' ' && !matches!(ch, '\u{fffe}' | '\u{ffff}')))
        })
    {
        issues.push(format!(
            "{path}: text contains characters forbidden in the required XML plist"
        ));
    }
}

fn check_xml(value: &Value, path: &str, issues: &mut Vec<String>) {
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

fn requires_xml(value: &Value) -> bool {
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

fn defaults_value(value: &Value) -> String {
    let (flag, text) = match value {
        Value::Boolean(value) => ("-bool", if *value { "TRUE" } else { "FALSE" }.to_owned()),
        Value::Integer(value) => ("-int", value.to_string()),
        Value::Real(value) => {
            let mut text = value.to_string();
            if !text.contains(['.', 'e', 'E']) {
                text.push_str(".0");
            }
            ("-float", text)
        }
        Value::String(value) => ("-string", value.clone()),
        Value::Date(value) => (
            "-date",
            value
                .to_xml_format()
                .replace('T', " ")
                .replace('Z', " +0000"),
        ),
        Value::Data(value) => ("-data", hex(value)),
        Value::Array(values) if values.is_empty() => return "-array".into(),
        Value::Dict(values) if values.is_empty() => return "-dict".into(),
        Value::Array(_) | Value::Dict(_) => return shell_quote(&openstep(value)),
        Value::Uid(_) => unreachable!("unsupported values were excluded before rendering"),
    };
    format!("{flag} {}", shell_quote(&text))
}

fn hex(bytes: &[u8]) -> String {
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        let _ = write!(output, "{byte:02x}");
    }
    output
}

fn openstep(value: &Value) -> String {
    match value {
        Value::String(value) => openstep_string(value),
        Value::Data(value) => format!("<{}>", hex(value)),
        Value::Array(values) => format!(
            "({})",
            values
                .iter()
                .map(|setting| openstep(&setting.value))
                .collect::<Vec<_>>()
                .join(", ")
        ),
        Value::Dict(values) => {
            let mut output = String::from("{");
            for (key, setting) in values {
                let _ = write!(
                    output,
                    " {} = {};",
                    openstep_string(key),
                    openstep(&setting.value)
                );
            }
            output.push_str(" }");
            output
        }
        _ => unreachable!("only string/data compounds use OpenStep"),
    }
}

fn openstep_string(value: &str) -> String {
    let mut output = String::from("\"");
    for ch in value.chars() {
        match ch {
            '\\' => output.push_str("\\\\"),
            '"' => output.push_str("\\\""),
            '\n' => output.push_str("\\n"),
            '\r' => output.push_str("\\r"),
            '\t' => output.push_str("\\t"),
            ch if ch.is_control() || !ch.is_ascii() => {
                for unit in ch.encode_utf16(&mut [0; 2]) {
                    let _ = write!(output, "\\U{unit:04X}");
                }
            }
            ch => output.push(ch),
        }
    }
    output.push('"');
    output
}

fn xml_dictionary(values: &BTreeMap<&str, &Setting>) -> Result<String> {
    let dictionary = values
        .iter()
        .map(|(key, setting)| Ok(((*key).to_owned(), setting.value.to_plist()?)))
        .collect::<Result<plist::Dictionary>>()?;
    let mut output = Vec::new();
    plist::Value::Dictionary(dictionary)
        .to_writer_xml(&mut output)
        .map_err(|error| Error::Invalid(format!("cannot encode native XML plist: {error}")))?;
    let output = String::from_utf8(output)
        .map_err(|error| Error::Invalid(format!("native XML plist was not UTF-8: {error}")))?;
    // Literal CR is normalized to LF by XML 1.0 readers; a character reference preserves it.
    Ok(output.replace('\r', "&#13;"))
}

fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

fn comment_text(value: &str) -> String {
    let mut output = String::new();
    for ch in value.chars() {
        if ch.is_control() || matches!(ch, '\u{2028}' | '\u{2029}') {
            output.extend(ch.escape_default());
        } else {
            output.push(ch);
        }
    }
    output
}

fn power_value_issue(key: &str, value: &Value) -> Option<String> {
    let Value::Integer(value) = value else {
        return Some("pmset requires an integer; no type coercion is performed".into());
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
        _ => return Some("key has no verified value-range mapping".into()),
    };
    Some(format!("unsupported pmset value {value}; expected {range}"))
}
