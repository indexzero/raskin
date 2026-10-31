//! The owner-approved Raskin schema, encoded using KDL 2.
use crate::model::{MAX_DEPTH, MAX_DOCUMENT_BYTES};
use crate::{
    Document, Domain, Error, Host, Properties, Reason, Result, Scope, Setting, Unavailable, Value,
};
use base64::{Engine, engine::general_purpose::STANDARD};
use kdl::{KdlDocument, KdlEntry, KdlNode, KdlValue};
use std::collections::{BTreeMap, BTreeSet};

/// Parse and validate a complete preference document without host access.
pub fn decode(input: &str, source_name: &str) -> Result<Document> {
    let decoder = Decoder { input, source_name };
    if input.len() > MAX_DOCUMENT_BYTES {
        return Err(decoder.at(0, "document exceeds the 128 MiB input limit"));
    }
    check_nesting(input)
        .map_err(|offset| decoder.at(offset, "document exceeds the nesting limit of 128"))?;
    let tree = KdlDocument::parse_v2(input).map_err(|error| {
        if let Some(diagnostic) = error.diagnostics.first() {
            let message = format!(
                "{}{}",
                diagnostic
                    .message
                    .as_deref()
                    .unwrap_or("invalid KDL 2 syntax"),
                diagnostic
                    .help
                    .as_ref()
                    .map(|help| format!("; {help}"))
                    .unwrap_or_default()
            );
            decoder.at(diagnostic.span.offset(), message)
        } else {
            decoder.at(0, error.to_string())
        }
    })?;
    let mut document = Document::default();
    let mut groups = BTreeSet::new();
    for node in tree.nodes() {
        let name = node.name().value();
        if name != "unavailable" && !groups.insert(name.to_owned()) {
            return Err(decoder.error(node, format!("duplicate root group {name:?}")));
        }
        match name {
            "raskin" => {
                decoder.no_annotation(node)?;
                if node.entries().len() != 1
                    || node.children().is_some()
                    || node.entries()[0].name().is_some()
                    || node.entries()[0].ty().is_some()
                {
                    return Err(decoder.error(node, "expected exactly raskin 1"));
                }
                if node.entries()[0].value() != &KdlValue::Integer(1) {
                    return Err(
                        decoder.error(node, "unsupported Raskin schema version; expected raskin 1")
                    );
                }
            }
            "defaults" => {
                decoder.group(node)?;
                for child in node.children().expect("group validated").nodes() {
                    decoder.namespace(child, "", &mut document)?;
                }
            }
            "pmset" => {
                decoder.group(node)?;
                for profile in node.children().expect("group validated").nodes() {
                    decoder.group(profile)?;
                    let mut values = BTreeMap::new();
                    for child in profile.children().expect("group validated").nodes() {
                        let setting = decoder.setting(child, 0)?;
                        if !setting.value.is_scalar() {
                            return Err(decoder.error(child, "pmset values must be scalars"));
                        }
                        decoder.insert(&mut values, child, setting)?;
                    }
                    if document
                        .power
                        .insert(profile.name().value().to_owned(), values)
                        .is_some()
                    {
                        return Err(decoder.error(profile, "duplicate power profile"));
                    }
                }
            }
            "unavailable" => {
                decoder.no_annotation(node)?;
                decoder.check_properties(node)?;
                let args = arguments(node);
                if args.len() != 1 || args[0].ty().is_some() || node.children().is_some() {
                    return Err(decoder.error(
                        node,
                        "unavailable needs one source string and a reason property",
                    ));
                }
                let source = args[0]
                    .value()
                    .as_string()
                    .ok_or_else(|| decoder.error(node, "unavailable source must be a string"))?;
                let props = named(node);
                if props.len() != 1
                    || props[0].name().map(|n| n.value()) != Some("reason")
                    || props[0].ty().is_some()
                {
                    return Err(
                        decoder.error(node, "unavailable needs exactly one reason property")
                    );
                }
                let reason = match props[0].value().as_string() {
                    Some("permission-denied") => Reason::PermissionDenied,
                    Some("unsupported") => Reason::Unsupported,
                    Some("missing") => Reason::Missing,
                    Some("read-failed") => Reason::ReadFailed,
                    _ => return Err(decoder.error(node, "unknown unavailable reason")),
                };
                document.unavailable.push(Unavailable {
                    source: source.to_owned(),
                    reason,
                });
            }
            _ => return Err(decoder.error(node, format!("unknown command group {name:?}"))),
        }
    }
    if !groups.contains("raskin") {
        return Err(decoder.at(0, "missing raskin 1 schema header"));
    }
    document
        .validate()
        .map_err(|error| decoder.at(0, error.to_string()))?;
    document.unavailable.sort();
    Ok(document)
}

struct Decoder<'a> {
    input: &'a str,
    source_name: &'a str,
}
impl Decoder<'_> {
    fn at(&self, mut offset: usize, message: impl Into<String>) -> Error {
        offset = offset.min(self.input.len());
        while !self.input.is_char_boundary(offset) {
            offset -= 1;
        }
        let prefix = &self.input[..offset];
        let line = prefix.bytes().filter(|&c| c == b'\n').count() + 1;
        let column = prefix.rsplit('\n').next().unwrap_or("").chars().count() + 1;
        Error::Document {
            source_name: self.source_name.into(),
            line,
            column,
            message: message.into(),
        }
    }
    fn error(&self, node: &KdlNode, message: impl Into<String>) -> Error {
        self.at(node.span().offset(), message)
    }
    fn no_annotation(&self, node: &KdlNode) -> Result<()> {
        if node.ty().is_some() {
            return Err(self.error(node, "unexpected node type annotation"));
        }
        Ok(())
    }
    fn group(&self, node: &KdlNode) -> Result<()> {
        self.no_annotation(node)?;
        if !node.entries().is_empty() || node.children().is_none() {
            return Err(self.error(
                node,
                "expected a group with children and no values or properties",
            ));
        }
        Ok(())
    }
    fn check_properties(&self, node: &KdlNode) -> Result<()> {
        let mut names = BTreeSet::new();
        for entry in named(node) {
            let name = entry.name().expect("named entry").value();
            if !names.insert(name) {
                return Err(self.error(node, format!("duplicate property {name:?}")));
            }
        }
        Ok(())
    }
    fn namespace(&self, node: &KdlNode, prefix: &str, document: &mut Document) -> Result<()> {
        self.no_annotation(node)?;
        self.check_properties(node)?;
        if !arguments(node).is_empty() || node.children().is_none() {
            return Err(self.error(
                node,
                "namespace nodes need children and no positional values",
            ));
        }
        let name = if prefix.is_empty() {
            node.name().value().to_owned()
        } else {
            format!("{prefix}.{}", node.name().value())
        };
        let mut domain = Domain::user(name.clone());
        for entry in named(node) {
            if entry.ty().is_some() {
                return Err(self.error(node, "scope properties cannot have type annotations"));
            }
            let value = entry
                .value()
                .as_string()
                .ok_or_else(|| self.error(node, "scope properties must be strings"))?;
            match entry.name().expect("named entry").value() {
                "scope" => {
                    domain.scope = match value {
                        "user" => Scope::User,
                        "system" => Scope::System,
                        _ => return Err(self.error(node, "scope must be user or system")),
                    }
                }
                "host" => {
                    domain.host = match value {
                        "any" => Host::Any,
                        "current" => Host::Current,
                        _ => return Err(self.error(node, "host must be any or current")),
                    }
                }
                "container" => domain.container = Some(value.to_owned()),
                key => return Err(self.error(node, format!("unknown domain property {key:?}"))),
            }
        }
        let mut properties = BTreeMap::new();
        let mut branches = Vec::new();
        for child in node.children().expect("namespace children").nodes() {
            if child.ty().is_some() || !arguments(child).is_empty() {
                self.insert(&mut properties, child, self.setting(child, 0)?)?;
            } else {
                branches.push(child);
            }
        }
        if properties.is_empty() && !named(node).is_empty() {
            return Err(self.error(
                node,
                "scope belongs on a domain with direct preference values",
            ));
        }
        if !properties.is_empty() && document.domains.insert(domain, properties).is_some() {
            return Err(self.error(node, format!("duplicate scoped domain {name:?}")));
        }
        for child in branches {
            self.namespace(child, &name, document)?;
        }
        Ok(())
    }
    fn insert(&self, map: &mut Properties, node: &KdlNode, value: Setting) -> Result<()> {
        let key = node.name().value().to_owned();
        if map.insert(key.clone(), value).is_some() {
            return Err(self.error(node, format!("duplicate preference/dictionary key {key:?}")));
        }
        Ok(())
    }
    fn setting(&self, node: &KdlNode, depth: usize) -> Result<Setting> {
        if depth > MAX_DEPTH {
            return Err(self.error(node, "value nesting limit exceeded"));
        }
        self.check_properties(node)?;
        let mut metadata = BTreeMap::new();
        for entry in named(node) {
            if entry.ty().is_some() {
                return Err(self.error(node, "metadata properties do not accept type annotations"));
            }
            metadata.insert(
                entry.name().expect("named entry").value().to_owned(),
                entry.value().clone(),
            );
        }
        let args = arguments(node);
        let value = match node.ty().map(|ty| ty.value()) {
            Some("dict") => {
                if !args.is_empty() || node.children().is_none() {
                    return Err(self.error(node, "a (dict) needs children, including {} when empty, and no positional values"));
                }
                let mut properties = BTreeMap::new();
                for child in node.children().expect("dictionary children").nodes() {
                    self.insert(&mut properties, child, self.setting(child, depth + 1)?)?;
                }
                Value::Dict(properties)
            }
            Some("array") => {
                if !args.is_empty() && node.children().is_some() {
                    return Err(self.error(
                        node,
                        "an array cannot mix positional values and a children block",
                    ));
                }
                if args.is_empty() {
                    let children = node
                        .children()
                        .ok_or_else(|| self.error(node, "an empty array needs explicit {}"))?;
                    let mut items = Vec::new();
                    for child in children.nodes() {
                        if child.name().value() != "-" {
                            return Err(self.error(child, "array child nodes must be named -"));
                        }
                        items.push(self.setting(child, depth + 1)?);
                    }
                    Value::Array(items)
                } else {
                    Value::Array(
                        args.iter()
                            .map(|entry| self.scalar(node, entry).map(Setting::new))
                            .collect::<Result<_>>()?,
                    )
                }
            }
            Some(annotation) => {
                return Err(self.error(
                    node,
                    format!("unknown collection annotation {annotation:?}"),
                ));
            }
            None => {
                if args.len() != 1 || node.children().is_some() {
                    return Err(self.error(node, "a scalar has exactly one value and no children; annotate collections (dict) or (array)"));
                }
                self.scalar(node, args[0])?
            }
        };
        value
            .validate(depth)
            .map_err(|error| self.error(node, error.to_string()))?;
        Ok(Setting { value, metadata })
    }
    fn scalar(&self, node: &KdlNode, entry: &KdlEntry) -> Result<Value> {
        match entry.ty().map(|ty|ty.value()) {
            Some("base64") => entry.value().as_string()
                .ok_or_else(|| self.error(node,"(base64) requires a string"))
                .and_then(|s| STANDARD.decode(s).map(Value::Data).map_err(|e|self.error(node,format!("invalid base64: {e}")))),
            Some("date-time") => entry.value().as_string()
                .ok_or_else(||self.error(node,"(date-time) requires an RFC 3339 string"))
                .and_then(|s| plist::Date::from_xml_format(s).map(Value::Date).map_err(|e|self.error(node,format!("invalid date-time: {e}")))),
            Some("plist-uid") => entry.value().as_integer()
                .and_then(|v|u64::try_from(v).ok()).map(Value::Uid)
                .ok_or_else(||self.error(node,"(plist-uid) requires an unsigned 64-bit integer")),
            Some(annotation) => Err(self.error(node,format!("unknown scalar annotation {annotation:?}"))),
            None => match entry.value() {
                KdlValue::String(v) => Ok(Value::String(v.clone())),
                KdlValue::Bool(v) => Ok(Value::Boolean(*v)),
                KdlValue::Integer(v) => Ok(Value::Integer(*v)),
                KdlValue::Float(v) => Ok(Value::Real(*v)),
                KdlValue::Null => Err(self.error(node,"null is not a native preference value; use an unavailable record for a capture omission")),
            },
        }
    }
}
fn arguments(node: &KdlNode) -> Vec<&KdlEntry> {
    node.entries()
        .iter()
        .filter(|e| e.name().is_none())
        .collect()
}
fn named(node: &KdlNode) -> Vec<&KdlEntry> {
    node.entries()
        .iter()
        .filter(|e| e.name().is_some())
        .collect()
}

/// Return stable KDL with shared namespace prefixes and explicit value types.
pub fn encode(document: &Document) -> Result<String> {
    document.validate()?;
    let mut output = String::from("raskin 1\n");
    if !document.domains.is_empty() {
        let mut root = Branch::default();
        for (domain, properties) in &document.domains {
            let mut branch = &mut root;
            for segment in domain.name.split('.') {
                branch = branch.children.entry(segment.to_owned()).or_default();
            }
            branch.domains.push((domain, properties));
        }
        output.push_str("\ndefaults {\n");
        write_branches(&mut output, &root, 1);
        output.push_str("}\n");
    }
    if !document.power.is_empty() {
        output.push_str("\npmset {\n");
        for (profile, properties) in &document.power {
            output.push_str(&format!("    {} {{", ident(profile)));
            if properties.is_empty() {
                output.push_str("}\n");
            } else {
                output.push('\n');
                for (key, value) in properties {
                    write_setting(&mut output, key, value, 2);
                }
                output.push_str("    }\n");
            }
        }
        output.push_str("}\n");
    }
    let mut unavailable = document.unavailable.clone();
    unavailable.sort();
    for record in unavailable {
        output.push_str(&format!(
            "\nunavailable {} reason={}\n",
            string(&record.source),
            string(record.reason.as_str())
        ));
    }
    if output.len() > MAX_DOCUMENT_BYTES {
        return Err(Error::Invalid(
            "encoded document exceeds 128 MiB limit".into(),
        ));
    }
    Ok(output)
}
#[derive(Default)]
struct Branch<'a> {
    children: BTreeMap<String, Branch<'a>>,
    domains: Vec<(&'a Domain, &'a Properties)>,
}
fn write_branches(output: &mut String, branch: &Branch<'_>, depth: usize) {
    let indent = "    ".repeat(depth);
    for (name, child) in &branch.children {
        if child.domains.is_empty() {
            output.push_str(&format!("{indent}{} {{\n", ident(name)));
            write_branches(output, child, depth + 1);
            output.push_str(&format!("{indent}}}\n"));
        } else {
            for (index, (domain, properties)) in child.domains.iter().enumerate() {
                output.push_str(&format!("{indent}{}", ident(name)));
                if domain.scope == Scope::System {
                    output.push_str(" scope=\"system\"");
                }
                if domain.host == Host::Current {
                    output.push_str(" host=\"current\"");
                }
                if let Some(container) = &domain.container {
                    output.push_str(&format!(" container={}", string(container)));
                }
                output.push_str(" {\n");
                for (key, value) in *properties {
                    write_setting(output, key, value, depth + 1);
                }
                if index == 0 {
                    write_branches(output, child, depth + 1);
                }
                output.push_str(&format!("{indent}}}\n"));
            }
        }
    }
}
fn write_setting(output: &mut String, key: &str, setting: &Setting, depth: usize) {
    let indent = "    ".repeat(depth);
    let metadata = setting
        .metadata
        .iter()
        .map(|(name, value)| format!(" {}={}", ident(name), meta(value)))
        .collect::<String>();
    match &setting.value {
        Value::Dict(values) => {
            output.push_str(&format!("{indent}(dict){}{metadata} {{", ident(key)));
            if values.is_empty() {
                output.push_str("}\n");
            } else {
                output.push('\n');
                for (key, value) in values {
                    write_setting(output, key, value, depth + 1);
                }
                output.push_str(&format!("{indent}}}\n"));
            }
        }
        Value::Array(values) => {
            output.push_str(&format!("{indent}(array){}", ident(key)));
            if values.is_empty() {
                output.push_str(&format!("{metadata} {{}}\n"));
            } else if values
                .iter()
                .all(|v| v.value.is_scalar() && v.metadata.is_empty())
            {
                for value in values {
                    output.push_str(&format!(" {}", scalar(&value.value)));
                }
                output.push_str(&format!("{metadata}\n"));
            } else {
                output.push_str(&format!("{metadata} {{\n"));
                for value in values {
                    write_setting(output, "-", value, depth + 1);
                }
                output.push_str(&format!("{indent}}}\n"));
            }
        }
        value => output.push_str(&format!(
            "{indent}{} {}{metadata}\n",
            ident(key),
            scalar(value)
        )),
    }
}
fn scalar(value: &Value) -> String {
    match value {
        Value::Boolean(true) => "#true".into(),
        Value::Boolean(false) => "#false".into(),
        Value::Integer(v) => v.to_string(),
        Value::Real(v) => real(*v),
        Value::String(v) => string(v),
        Value::Date(v) => format!("(date-time){}", string(&v.to_xml_format())),
        Value::Data(v) => format!("(base64){}", string(&STANDARD.encode(v))),
        Value::Uid(v) => format!("(plist-uid){v}"),
        Value::Array(_) | Value::Dict(_) => unreachable!("collections are encoded as nodes"),
    }
}
fn meta(value: &KdlValue) -> String {
    match value {
        KdlValue::String(v) => string(v),
        KdlValue::Integer(v) => v.to_string(),
        KdlValue::Float(v) => real(*v),
        KdlValue::Bool(true) => "#true".into(),
        KdlValue::Bool(false) => "#false".into(),
        KdlValue::Null => "#null".into(),
    }
}
fn real(value: f64) -> String {
    if value.is_nan() {
        "#nan".into()
    } else if value == f64::INFINITY {
        "#inf".into()
    } else if value == f64::NEG_INFINITY {
        "#-inf".into()
    } else {
        format!("{value:?}")
    }
}
fn ident(value: &str) -> String {
    if value == "-"
        || (!value.is_empty()
            && value.starts_with(|c: char| c.is_ascii_alphabetic() || c == '_')
            && value
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.'))
            && !matches!(value, "true" | "false" | "null" | "inf" | "nan"))
    {
        value.to_owned()
    } else {
        string(value)
    }
}
fn string(value: &str) -> String {
    let mut output = String::from("\"");
    for ch in value.chars() {
        match ch {
            '"' => output.push_str("\\\""),
            '\\' => output.push_str("\\\\"),
            '\n' => output.push_str("\\n"),
            '\r' => output.push_str("\\r"),
            '\t' => output.push_str("\\t"),
            c if c.is_control() || matches!(c, '\u{2028}' | '\u{2029}' | '\u{feff}') => {
                output.push_str(&format!("\\u{{{:x}}}", c as u32))
            }
            c => output.push(c),
        }
    }
    output.push('"');
    output
}

// Bound recursion before the recursive KDL parser sees input. Skip KDL strings
// (including multiline/raw strings) and nested comments, not arbitrary braces in data.
fn check_nesting(input: &str) -> std::result::Result<(), usize> {
    let bytes = input.as_bytes();
    let (mut i, mut depth, mut comment) = (0, 0usize, 0usize);
    while i < bytes.len() {
        if comment > 0 {
            if bytes[i..].starts_with(b"/*") {
                comment += 1;
                if comment > MAX_DEPTH {
                    return Err(i);
                }
                i += 2;
            } else if bytes[i..].starts_with(b"*/") {
                comment -= 1;
                i += 2;
            } else {
                i += 1;
            }
            continue;
        }
        if bytes[i..].starts_with(b"//") {
            while i < bytes.len() && bytes[i] != b'\n' && bytes[i] != b'\r' {
                i += 1;
            }
            continue;
        }
        if bytes[i..].starts_with(b"/*") {
            comment = 1;
            i += 2;
            continue;
        }
        let start = i;
        let mut hashes = 0;
        if bytes[i] == b'#' {
            while i < bytes.len() && bytes[i] == b'#' {
                hashes += 1;
                i += 1;
            }
        }
        if i < bytes.len() && bytes[i] == b'"' {
            let quotes = if bytes[i..].starts_with(b"\"\"\"") {
                3
            } else {
                1
            };
            i += quotes;
            while i < bytes.len() {
                if hashes == 0 && bytes[i] == b'\\' {
                    i = (i + 2).min(bytes.len());
                    continue;
                }
                if bytes[i..].starts_with(&vec![b'"'; quotes]) {
                    let end = i + quotes;
                    if end + hashes <= bytes.len()
                        && bytes[end..end + hashes].iter().all(|c| *c == b'#')
                    {
                        i = end + hashes;
                        break;
                    }
                }
                i += 1;
            }
            continue;
        }
        i = start;
        match bytes[i] {
            b'{' => {
                depth += 1;
                if depth > MAX_DEPTH {
                    return Err(i);
                }
            }
            b'}' => depth = depth.saturating_sub(1),
            _ => {}
        }
        i += 1;
    }
    Ok(())
}
