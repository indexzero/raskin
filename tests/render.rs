//! These tests inspect generated text and parse plist DATA only.
//! They never execute scripts, launch shells, or call native settings commands.
use std::collections::BTreeMap;
use std::time::{Duration, UNIX_EPOCH};

use raskin::render::{GenerateOptions, Generated, generate};
use raskin::{
    Document, Domain, Error, Host, Properties, Reason, Scope, Setting, Unavailable, Value,
};

fn properties(values: impl IntoIterator<Item = (&'static str, Value)>) -> Properties {
    values
        .into_iter()
        .map(|(key, value)| (key.into(), value.into()))
        .collect()
}

fn document(values: impl IntoIterator<Item = (&'static str, Value)>) -> Document {
    Document {
        domains: [(Domain::user("com.example.render"), properties(values))].into(),
        ..Document::default()
    }
}

fn array(values: impl IntoIterator<Item = Value>) -> Value {
    Value::Array(values.into_iter().map(Setting::new).collect())
}

fn date(value: &str) -> Value {
    Value::Date(plist::Date::from_xml_format(value).unwrap())
}

fn strict_options() -> GenerateOptions {
    GenerateOptions {
        skip_unsupported: false,
    }
}

fn rendered(document: &Document) -> Generated {
    generate(document, GenerateOptions::default()).unwrap()
}

fn partial(document: &Document) -> Generated {
    generate(
        document,
        GenerateOptions {
            skip_unsupported: true,
        },
    )
    .unwrap()
}

/// Decode the renderer's single-quoted string notation as DATA, without a shell.
fn quoted_data(argument: &str) -> String {
    let inner = argument
        .strip_prefix('\'')
        .unwrap()
        .strip_suffix('\'')
        .unwrap();
    let parts = inner.split("'\\''").collect::<Vec<_>>();
    assert!(parts.iter().all(|part| !part.contains('\'')));
    parts.join("'")
}

fn openstep_payload(script: &str, key: &str) -> String {
    let prefix = format!("/usr/bin/defaults write 'com.example.render' '{key}' ");
    let line = script
        .lines()
        .find_map(|line| line.strip_prefix(&prefix))
        .unwrap();
    quoted_data(line)
}

/// Extract an embedded here-document as text, without interpreting it.
fn xml_payload(script: &str) -> (&str, &str) {
    let (_, tail) = script.split_once(" <<'").unwrap();
    let (delimiter, tail) = tail.split_once("'\n").unwrap();
    let closing = format!("\n{delimiter}\n");
    let (payload, _) = tail.split_once(&closing).unwrap();
    assert!(!payload.contains(delimiter));
    (delimiter, payload)
}

fn parsed_xml(script: &str) -> Value {
    let (_, payload) = xml_payload(script);
    Value::from_plist(plist::Value::from_reader_xml(payload.as_bytes()).unwrap()).unwrap()
}

#[test]
fn scalar_snapshot_preserves_flags_padding_dates_and_signed_zero() {
    let document = document([
        ("bytes", Value::Data(vec![0, 1, 15, 255])),
        ("date", date("2026-10-30T15:04:05Z")),
        ("disabled", Value::Boolean(false)),
        ("enabled", Value::Boolean(true)),
        ("float", Value::Real(1.0)),
        ("integer", Value::Integer(1)),
        ("negative-zero", Value::Real(-0.0)),
        ("text", Value::String("hello".into())),
    ]);
    let output = rendered(&document);
    assert_eq!(output.script, include_str!("fixtures/render/scalars.macos"));
    assert!(output.diagnostics.is_empty());
}

#[test]
fn empty_collections_use_distinct_native_flags() {
    let output = rendered(&document([
        ("array", array([])),
        ("dict", Value::Dict(Properties::new())),
        ("empty-data", Value::Data(vec![])),
        ("empty-string", Value::String(String::new())),
    ]));
    assert!(output.script.contains("'array' -array\n"));
    assert!(output.script.contains("'dict' -dict\n"));
    assert!(output.script.contains("'empty-data' -data ''\n"));
    assert!(output.script.contains("'empty-string' -string ''\n"));
    assert!(!output.script.contains(" import "));
}

#[test]
fn openstep_snapshot_and_string_collections_preserve_structure() {
    let value = Value::Dict(properties([
        ("bytes", Value::Data(vec![0, 1, 15])),
        ("empty-data", Value::Data(vec![])),
        ("empty-dict", Value::Dict(Properties::new())),
        ("empty-list", array([])),
        (
            "nested",
            array([
                Value::Dict(properties([("name", Value::String("one".into()))])),
                array([Value::String("two".into())]),
            ]),
        ),
        (
            "repeated",
            array([
                Value::String("123".into()),
                Value::String("123".into()),
                Value::String("TRUE".into()),
                Value::String("(x)".into()),
            ]),
        ),
        ("singleton", array([Value::String("only".into())])),
    ]));
    let output = rendered(&document([("tree", value.clone())]));
    assert!(!output.script.contains(" import "));
    let payload = openstep_payload(&output.script, "tree");
    assert_eq!(
        payload,
        include_str!("fixtures/render/compound.openstep").trim_end()
    );
    // plist 1.10.3's ASCII reader treats bare <hex> tokens as strings.
    // The exact fixture above checks native data notation. Parse the remaining
    // string-only collections to verify structure, order, and duplicate values.
    let Value::Dict(mut strings) = value else {
        unreachable!()
    };
    strings.remove("bytes");
    strings.remove("empty-data");
    let strings = Value::Dict(strings);
    let output = rendered(&document([("tree", strings.clone())]));
    let payload = openstep_payload(&output.script, "tree");
    let parsed = plist::Value::from_reader_ascii(payload.as_bytes()).unwrap();
    assert_eq!(Value::from_plist(parsed).unwrap(), strings);
}

#[test]
fn both_quoting_layers_keep_hostile_text_as_openstep_data() {
    let text = "你好 😀\n$HOME $(ignored) `ignored` ' \" \\ \u{1b}\t\r\u{85}";
    let key = "key'\" = (not, structure);\n$HOME";
    let mut nested = Properties::new();
    nested.insert(key.into(), Value::String(text.into()).into());
    let value = array([
        Value::String(text.into()),
        Value::Dict(nested),
        Value::String("\\U1234".into()),
    ]);
    let output = rendered(&document([("quoted", value.clone())]));
    let payload = openstep_payload(&output.script, "quoted");
    assert!(output.script.contains("'\\''"));
    assert!(payload.contains("\\UD83D\\UDE00"));
    assert!(payload.contains("\\U001B"));
    assert!(!payload.contains('\u{1b}'));
    let parsed = plist::Value::from_reader_ascii(payload.as_bytes()).unwrap();
    assert_eq!(Value::from_plist(parsed).unwrap(), value);
}

#[test]
fn scalar_shell_arguments_preserve_quotes_newlines_unicode_and_metacharacters() {
    let mut document = Document::default();
    document.domains.insert(
        Domain::user("com.example.render"),
        [(
            "quo'te\nkey".into(),
            Value::String("你好\n$HOME $(ignored) `ignored` ' \" \\ \u{1b}\t".into()).into(),
        )]
        .into(),
    );
    let output = rendered(&document);
    let expected = "/usr/bin/defaults write 'com.example.render' 'quo'\\''te\nkey' -string '你好\n$HOME $(ignored) `ignored` '\\'' \" \\ \u{1b}\t'\n";
    assert!(output.script.ends_with(expected));
    let (_, value) = output.script.split_once(" -string ").unwrap();
    assert_eq!(
        quoted_data(value.strip_suffix('\n').unwrap()),
        "你好\n$HOME $(ignored) `ignored` ' \" \\ \u{1b}\t"
    );
}

#[test]
fn typed_compounds_use_one_complete_domain_dictionary_and_round_trip() {
    let compound = array([
        Value::Boolean(true),
        Value::Boolean(false),
        Value::Integer(i64::MIN as i128),
        Value::Integer(i64::MAX as i128),
        Value::Real(-0.0),
        Value::Real(1.0),
        Value::Real(f64::MIN_POSITIVE),
        date("2026-10-30T15:04:05Z"),
        Value::Data(vec![0, 1, 15, 255]),
        Value::String("你好 😀 <&> '\n$HOME `ignored`\rnext\r\nline".into()),
        array([]),
        Value::Dict(properties([("nested", Value::Boolean(true))])),
    ]);
    let mut document = document([
        ("compound", compound),
        ("sibling", Value::String("retained scalar".into())),
        ("empty", Value::Dict(Properties::new())),
    ]);
    document.domains.values_mut().next().unwrap().insert(
        "carriage\rreturn".into(),
        Value::String("kept\rverbatim".into()).into(),
    );
    let output = rendered(&document);
    assert_eq!(output.script.matches(" import ").count(), 2); // explanatory comment and operation
    assert!(
        output
            .script
            .contains("/usr/bin/defaults import 'com.example.render' - <<'RASKIN_PLIST_1'")
    );
    assert!(!output.script.contains(" write "));
    assert!(
        output
            .script
            .contains("destination keys absent from this dictionary")
    );
    assert!(output.script.contains("&#13;"));
    let expected = Value::Dict(document.domains.values().next().unwrap().clone());
    assert_eq!(parsed_xml(&output.script), expected);
}

#[test]
fn heredoc_delimiter_avoids_payload_collisions_and_disables_expansion() {
    let value = array([
        Value::Boolean(true),
        Value::String("\nRASKIN_PLIST_1\nRASKIN_PLIST_2\n$HOME $(ignored) `ignored`\n".into()),
    ]);
    let document = document([("payload", value)]);
    let output = rendered(&document);
    let (delimiter, payload) = xml_payload(&output.script);
    assert_eq!(delimiter, "RASKIN_PLIST_3");
    assert!(payload.contains("RASKIN_PLIST_1"));
    assert!(payload.contains("RASKIN_PLIST_2"));
    assert!(output.script.contains("<<'RASKIN_PLIST_3'"));
    assert_eq!(
        parsed_xml(&output.script),
        Value::Dict(document.domains.values().next().unwrap().clone())
    );
}

#[test]
fn finite_reals_round_trip_the_written_float_text_including_signed_zero() {
    for number in [
        0.0,
        -0.0,
        1.0,
        -1.0,
        f64::MAX,
        f64::MIN_POSITIVE,
        f64::from_bits(1),
    ] {
        let output = rendered(&document([("number", Value::Real(number))]));
        let (_, argument) = output.script.split_once(" -float ").unwrap();
        let value = quoted_data(argument.trim_end_matches('\n'));
        assert_eq!(value.parse::<f64>().unwrap().to_bits(), number.to_bits());
    }
}

#[test]
fn dates_are_24_hour_utc_and_fractional_dates_are_never_truncated() {
    let output = rendered(&document([
        ("offset", date("2026-10-30T20:34:05+05:30")),
        ("before-epoch", date("1969-12-31T23:59:59Z")),
    ]));
    assert!(
        output
            .script
            .contains("'offset' -date '2026-10-30 15:04:05 +0000'")
    );
    assert!(
        output
            .script
            .contains("'before-epoch' -date '1969-12-31 23:59:59 +0000'")
    );
    for text in ["2026-10-30T15:04:05.000000001Z", "1969-12-31T23:59:59.25Z"] {
        for value in [date(text), array([date(text)])] {
            let document = document([("fraction", value)]);
            let error = generate(&document, strict_options()).unwrap_err();
            assert!(error.to_string().contains("fractional-second date"));
            let partial = partial(&document);
            assert_eq!(partial.diagnostics.len(), 1);
            assert!(!partial.script.contains("/usr/bin/defaults"));
        }
    }
}

#[test]
fn library_created_unformattable_dates_get_errors_without_panics() {
    for time in [
        UNIX_EPOCH + Duration::from_secs(253_402_300_800),
        UNIX_EPOCH - Duration::from_secs(62_167_219_201),
    ] {
        let document = document([("date", Value::Date(time.into()))]);
        let error = generate(&document, strict_options()).unwrap_err();
        assert!(matches!(error, Error::Invalid(_)));
        assert!(error.to_string().contains("RFC 3339 year range"));
    }
}

#[test]
fn unsupported_types_skip_whole_top_level_keys_and_account_for_each() {
    let document = document([
        (
            "array-uid",
            array([
                Value::String("do not retain this element".into()),
                Value::Uid(7),
            ]),
        ),
        (
            "dict-nan",
            Value::Dict(properties([
                ("valid", Value::Integer(1)),
                ("invalid", Value::Real(f64::NAN)),
            ])),
        ),
        ("fraction", date("2026-10-30T15:04:05.125Z")),
        ("infinity", Value::Real(f64::INFINITY)),
        ("negative-infinity", Value::Real(f64::NEG_INFINITY)),
        ("uid", Value::Uid(u64::MAX)),
        ("unsigned", Value::Integer(i64::MAX as i128 + 1)),
        ("unsigned-max", Value::Integer(u64::MAX as i128)),
        ("valid", Value::String("keep".into())),
        ("valid-typed", array([Value::Boolean(true)])),
    ]);
    let error = generate(&document, strict_options()).unwrap_err();
    let Error::Unsupported(failures) = error else {
        panic!("expected unsupported settings")
    };
    let error = failures.diagnostics.join("\n");
    assert!(error.contains("array-uid\"[1]"));
    assert!(error.contains("dict-nan\"[\"invalid\"]"));
    assert!(error.contains("signed 64-bit"));
    assert!(error.contains("nonfinite"));
    let output = partial(&document);
    assert_eq!(output.diagnostics.len(), 8);
    assert!(
        output
            .script
            .contains("# Partial output: 8 unsupported items skipped.")
    );
    assert_eq!(
        parsed_xml(&output.script),
        Value::Dict(properties([
            ("valid", Value::String("keep".into())),
            ("valid-typed", array([Value::Boolean(true)])),
        ]))
    );
    assert!(!xml_payload(&output.script).1.contains("do not retain"));
    assert!(output.diagnostics.iter().all(|diagnostic| {
        output
            .script
            .lines()
            .any(|line| line == format!("# {diagnostic}"))
    }));
}

#[test]
fn fractional_and_large_integer_rejection_is_recursive_in_xml() {
    for value in [
        date("2026-10-30T15:04:05.25Z"),
        Value::Integer(u64::MAX as i128),
    ] {
        let nested = Value::Dict(properties([
            ("valid", Value::Boolean(true)),
            ("array", array([Value::Dict(properties([("bad", value)]))])),
        ]));
        let document = document([
            ("nested", nested),
            ("retained", array([Value::Boolean(false)])),
        ]);
        assert!(generate(&document, strict_options()).is_err());
        let output = partial(&document);
        assert_eq!(output.diagnostics.len(), 1);
        assert!(output.diagnostics[0].contains("[\"array\"][0][\"bad\"]"));
        assert_eq!(
            parsed_xml(&output.script),
            Value::Dict(properties([("retained", array([Value::Boolean(false)]))]))
        );
    }
}

#[test]
fn controls_use_openstep_when_possible_and_diagnose_required_xml() {
    let control = Value::String("before\u{1b}after".into());
    let standalone = rendered(&document([("control", control.clone())]));
    assert!(standalone.script.contains(" -string "));
    let openstep = rendered(&document([("control", array([control.clone()]))]));
    let payload = openstep_payload(&openstep.script, "control");
    assert_eq!(
        Value::from_plist(plist::Value::from_reader_ascii(payload.as_bytes()).unwrap()).unwrap(),
        array([control.clone()])
    );
    let xml = document([
        ("control", control.clone()),
        ("typed", array([Value::Boolean(true)])),
    ]);
    assert!(generate(&xml, strict_options()).is_err());
    let output = partial(&xml);
    assert_eq!(output.diagnostics.len(), 1);
    assert!(output.diagnostics[0].contains("forbidden in the required XML"));
    assert_eq!(
        parsed_xml(&output.script),
        Value::Dict(properties([("typed", array([Value::Boolean(true)]))]))
    );

    // Removing an unsupported XML-requiring key must not also lose a scalar
    // that no longer needs XML.
    let document = document([
        ("control", control),
        (
            "bad-compound",
            array([Value::Boolean(true), Value::String("\u{1b}".into())]),
        ),
    ]);
    let output = partial(&document);
    assert_eq!(output.diagnostics.len(), 1);
    assert!(output.script.contains("'control' -string "));
    assert!(!output.script.contains(" import "));
}

#[test]
fn nul_keys_and_values_are_never_rendered() {
    for value in [
        Value::String("bad\0text".into()),
        array([Value::String("bad\0text".into())]),
        Value::Dict([("bad\0key".into(), Value::String("value".into()).into())].into()),
    ] {
        let document = document([("bad", value)]);
        assert!(generate(&document, strict_options()).is_err());
        let output = partial(&document);
        assert_eq!(output.diagnostics.len(), 1);
        assert!(!output.script.contains('\0'));
        assert!(!output.script.contains("/usr/bin/defaults"));
    }
    let document = Document {
        domains: [(
            Domain::user("com.example.render"),
            [("bad\0key".into(), Value::Integer(1).into())].into(),
        )]
        .into(),
        ..Document::default()
    };
    assert!(generate(&document, strict_options()).is_err());
    assert!(!partial(&document).script.contains('\0'));
}

fn scope_cases() -> Vec<(Domain, &'static str)> {
    vec![
        (Domain::user("com.example.app"), "/usr/bin/defaults"),
        (
            Domain {
                host: Host::Current,
                ..Domain::user("com.example.current")
            },
            "/usr/bin/defaults -currentHost",
        ),
        (
            Domain {
                scope: Scope::System,
                ..Domain::user("com.example.system")
            },
            "sudo /usr/bin/defaults",
        ),
        (
            Domain {
                container: Some("com.example.container".into()),
                ..Domain::user("com.example.sandbox")
            },
            "/usr/bin/defaults",
        ),
        (
            Domain {
                scope: Scope::System,
                ..Domain::user("NSGlobalDomain")
            },
            "sudo /usr/bin/defaults",
        ),
        (
            Domain {
                container: Some("com.example.container".into()),
                ..Domain::user("NSGlobalDomain")
            },
            "/usr/bin/defaults",
        ),
        (Domain::user("NSGlobalDomain"), "/usr/bin/defaults"),
    ]
}

#[test]
fn all_supported_scope_targets_are_explicit_and_quoted() {
    let document = Document {
        domains: scope_cases()
            .into_iter()
            .map(|(domain, _)| (domain, properties([("key", Value::Integer(1))])))
            .collect(),
        ..Document::default()
    };
    let output = rendered(&document);
    for expected in [
        "/usr/bin/defaults write 'com.example.app' 'key' -int '1'",
        "/usr/bin/defaults -currentHost write 'com.example.current' 'key' -int '1'",
        "sudo /usr/bin/defaults write '/Library/Preferences/com.example.system' 'key' -int '1'",
        "/usr/bin/defaults write \"$HOME\"'/Library/Containers/com.example.container/Data/Library/Preferences/com.example.sandbox' 'key' -int '1'",
        "sudo /usr/bin/defaults write '/Library/Preferences/.GlobalPreferences' 'key' -int '1'",
        "/usr/bin/defaults write \"$HOME\"'/Library/Containers/com.example.container/Data/Library/Preferences/.GlobalPreferences' 'key' -int '1'",
        "/usr/bin/defaults write 'NSGlobalDomain' 'key' -int '1'",
    ] {
        assert!(output.script.contains(expected), "{expected}");
    }
    assert!(!output.script.contains("ByHost"));
    assert!(!output.script.contains("eval"));
}

#[test]
fn typed_imports_keep_scope_and_contain_only_native_plist_data() {
    for (domain, prefix) in scope_cases() {
        let document = Document {
            domains: [(domain, properties([("key", array([Value::Boolean(true)]))]))].into(),
            ..Document::default()
        };
        let output = rendered(&document);
        assert!(output.script.contains(&format!("{prefix} import ")));
        assert!(!output.script.contains(" write "));
        assert_eq!(
            parsed_xml(&output.script),
            Value::Dict(properties([("key", array([Value::Boolean(true)]))]))
        );
    }
}

#[test]
fn unsupported_scope_combinations_name_each_top_level_key() {
    for domain in [
        Domain {
            scope: Scope::System,
            host: Host::Current,
            ..Domain::user("com.example.app")
        },
        Domain {
            host: Host::Current,
            container: Some("com.example.container".into()),
            ..Domain::user("com.example.app")
        },
        Domain {
            scope: Scope::System,
            container: Some("com.example.container".into()),
            ..Domain::user("com.example.app")
        },
    ] {
        let document = Document {
            domains: [(
                domain,
                properties([("one", Value::Integer(1)), ("two", Value::Integer(2))]),
            )]
            .into(),
            ..Document::default()
        };
        assert!(generate(&document, strict_options()).is_err());
        let output = partial(&document);
        assert_eq!(output.diagnostics.len(), 2);
        assert!(
            output
                .diagnostics
                .iter()
                .any(|line| line.contains("key \"one\""))
        );
        assert!(
            output
                .diagnostics
                .iter()
                .any(|line| line.contains("key \"two\""))
        );
        assert!(!output.script.contains("/usr/bin/defaults"));
    }
}

#[test]
fn unsafe_domain_container_and_key_flags_are_rejected() {
    for name in [
        "-g",
        "/tmp/preferences",
        "~/preferences",
        "com.example/../../tmp",
        "com.example\\path",
        "com.example.$(ignored)",
        "com.example.\nignored",
        "com.example.\0nul",
    ] {
        let document = Document {
            domains: [(Domain::user(name), properties([("key", Value::Integer(1))]))].into(),
            ..Document::default()
        };
        assert!(generate(&document, strict_options()).is_err(), "{name:?}");
        match generate(
            &document,
            GenerateOptions {
                skip_unsupported: true,
            },
        ) {
            Ok(output) => assert!(!output.script.contains("/usr/bin/defaults")),
            Err(error) => assert!(matches!(error, Error::Invalid(_))),
        }
    }
    for container in [
        "../outside",
        "/tmp/outside",
        "com..example",
        "-app",
        "x'y",
        "x\0y",
    ] {
        let domain = Domain {
            container: Some(container.into()),
            ..Domain::user("com.example.app")
        };
        let document = Document {
            domains: [(domain, properties([("key", Value::Integer(1))]))].into(),
            ..Document::default()
        };
        assert!(generate(&document, strict_options()).is_err());
        assert!(!partial(&document).script.contains("/usr/bin/defaults"));
    }
    for value in [Value::String("value".into()), array([Value::Boolean(true)])] {
        let document = document([("-g", value)]);
        assert!(generate(&document, strict_options()).is_err());
        assert!(!partial(&document).script.contains("/usr/bin/defaults"));
    }
}

#[test]
fn metadata_remains_inert_even_when_it_looks_like_commands_or_source_identity() {
    let mut document = document([
        ("scalar", Value::Boolean(true)),
        ("compound", array([Value::String("value".into())])),
    ]);
    let expected = rendered(&document);
    for setting in document
        .domains
        .values_mut()
        .flat_map(|values| values.values_mut())
    {
        setting.metadata.insert(
            "command".into(),
            kdl::KdlValue::String("\nkillall Finder; $(ignored)".into()),
        );
        setting.metadata.insert(
            "host".into(),
            kdl::KdlValue::String("11111111-2222-3333-4444-555555555555".into()),
        );
        if let Value::Array(items) = &mut setting.value {
            items[0]
                .metadata
                .insert("restart".into(), kdl::KdlValue::String("cfprefsd".into()));
        }
    }
    assert_eq!(rendered(&document), expected);
}

#[test]
fn unavailable_observations_are_sorted_inert_comments() {
    let mut document = Document {
        unavailable: vec![
            Unavailable {
                source: "z-provider:\n/usr/bin/ignored\r\u{1b}\u{2028}\\\nnext".into(),
                reason: Reason::PermissionDenied,
            },
            Unavailable {
                source: "a-provider".into(),
                reason: Reason::Missing,
            },
        ],
        ..Document::default()
    };
    let output = rendered(&document);
    assert_eq!(output.diagnostics.len(), 2);
    assert!(output.script.lines().all(|line| line.starts_with('#')));
    assert!(!output.script.contains('\r'));
    assert!(!output.script.contains('\u{1b}'));
    assert!(!output.script.contains('\u{2028}'));
    assert!(output.script.contains("\\n/usr/bin/ignored\\r"));
    document.unavailable.reverse();
    assert_eq!(rendered(&document), output);
}

#[test]
fn power_profiles_keep_battery_charger_and_ups_separate() {
    let document = Document {
        power: [
            (
                "battery".into(),
                properties([
                    ("displaysleep", Value::Integer(2)),
                    ("sleep", Value::Integer(1)),
                ]),
            ),
            (
                "charger".into(),
                properties([
                    ("displaysleep", Value::Integer(120)),
                    ("sleep", Value::Integer(0)),
                ]),
            ),
            (
                "ups".into(),
                properties([("haltlevel", Value::Integer(20))]),
            ),
        ]
        .into(),
        ..Document::default()
    };
    let output = rendered(&document);
    assert_eq!(output.script, include_str!("fixtures/render/power.macos"));
    assert!(!output.script.contains("pmset -a"));
}

#[test]
fn power_unknown_profiles_keys_types_and_ranges_are_diagnostic() {
    let document = Document {
        power: [
            (
                "battery".into(),
                properties([
                    ("SleepServices", Value::Integer(1)),
                    ("networkoversleep", Value::Integer(1)),
                    ("hibernatefile", Value::String("/tmp/image".into())),
                    ("haltlevel", Value::Integer(20)),
                    ("sleep", Value::Integer(-1)),
                    ("womp", Value::Integer(2)),
                    ("powernap", Value::Boolean(true)),
                    ("hibernatemode", Value::Integer(5)),
                    ("highstandbythreshold", Value::Integer(101)),
                    ("standbydelayhigh", Value::Integer(i32::MAX as i128 + 1)),
                    ("displaysleep", Value::Integer(2)),
                ]),
            ),
            (
                "solar; ignored".into(),
                properties([("sleep", Value::Integer(1))]),
            ),
        ]
        .into(),
        ..Document::default()
    };
    assert!(generate(&document, strict_options()).is_err());
    let output = partial(&document);
    assert_eq!(output.diagnostics.len(), 11);
    let commands = output
        .script
        .lines()
        .filter(|line| line.starts_with("sudo "))
        .collect::<Vec<_>>();
    assert_eq!(commands, ["sudo /usr/bin/pmset -b 'displaysleep' '2'"]);
    assert!(!output.script.contains("pmset -a"));
}

#[test]
fn power_documented_range_edges_are_preserved() {
    let cases = [
        ("battery", "womp", 0),
        ("charger", "womp", 1),
        ("battery", "hibernatemode", 0),
        ("battery", "hibernatemode", 3),
        ("battery", "hibernatemode", 25),
        ("battery", "highstandbythreshold", 0),
        ("battery", "highstandbythreshold", 100),
        ("ups", "haltlevel", -1),
        ("ups", "haltlevel", 0),
        ("ups", "haltlevel", 100),
        ("ups", "haltafter", -1),
        ("ups", "haltremain", 0),
        ("battery", "displaysleep", 0),
        ("battery", "sleep", i32::MAX as i128),
        ("battery", "standbydelayhigh", 3600),
        ("battery", "standbydelaylow", 0),
        ("battery", "autopoweroffdelay", 60),
    ];
    for (profile, key, value) in cases {
        let document = Document {
            power: [(
                profile.into(),
                [(key.into(), Value::Integer(value).into())].into(),
            )]
            .into(),
            ..Document::default()
        };
        let output = rendered(&document);
        assert!(output.diagnostics.is_empty());
        assert!(output.script.contains(&format!("'{key}' '{value}'")));
    }
}

#[test]
fn generation_validates_library_created_documents_before_returning_output() {
    for value in [
        Value::Integer(i64::MIN as i128 - 1),
        Value::Integer(u64::MAX as i128 + 1),
    ] {
        let document = document([("out-of-range", value)]);
        assert!(matches!(
            generate(&document, strict_options()),
            Err(Error::Invalid(_))
        ));
        assert!(matches!(
            generate(
                &document,
                GenerateOptions {
                    skip_unsupported: true
                }
            ),
            Err(Error::Invalid(_))
        ));
    }
    let mut nested = Value::String("leaf".into());
    for _ in 0..129 {
        nested = array([nested]);
    }
    assert!(matches!(
        generate(&document([("too-deep", nested)]), strict_options()),
        Err(Error::Invalid(_))
    ));
}

#[test]
fn repeated_generation_is_deterministic_and_does_not_mutate_input() {
    let document = Document {
        domains: BTreeMap::from([
            (
                Domain::user("com.example.z"),
                properties([("key", array([Value::Boolean(true)]))]),
            ),
            (
                Domain::user("com.example.a"),
                properties([("key", Value::String("text".into()))]),
            ),
        ]),
        unavailable: vec![Unavailable {
            source: "provider:synthetic".into(),
            reason: Reason::Unsupported,
        }],
        ..Document::default()
    };
    let original = document.clone();
    let first = rendered(&document);
    for _ in 0..4 {
        assert_eq!(rendered(&document), first);
    }
    assert_eq!(document, original);
}

#[test]
fn tcp_keepalive_preserves_profiles_and_rejects_non_boolean_values() {
    let mut doc = Document::default();
    for (profile, value) in [("battery", 1), ("charger", 0), ("ups", 1)] {
        doc.power.insert(
            profile.into(),
            properties([("tcpkeepalive", Value::Integer(value))]),
        );
    }
    let script = rendered(&doc).script;
    assert!(script.contains("/usr/bin/pmset -b 'tcpkeepalive' '1'"));
    assert!(script.contains("/usr/bin/pmset -c 'tcpkeepalive' '0'"));
    assert!(script.contains("/usr/bin/pmset -u 'tcpkeepalive' '1'"));
    for value in [-1, 2] {
        let mut invalid = doc.clone();
        invalid
            .power
            .get_mut("battery")
            .unwrap()
            .insert("tcpkeepalive".into(), Value::Integer(value).into());
        let Error::Unsupported(failures) = generate(&invalid, strict_options()).unwrap_err() else {
            panic!("expected unsupported power value")
        };
        assert!(failures.diagnostics[0].contains("0 or 1"));
    }
}

#[test]
fn generation_counts_top_level_settings_once_per_reason_and_distinguishes_empty_sources() {
    let mut doc = document([
        (
            "mixed",
            array([Value::Uid(1), Value::Uid(2), Value::Real(f64::NAN)]),
        ),
        ("valid", Value::Boolean(true)),
    ]);
    doc.power.insert("unknown".into(), Default::default());
    let Error::Unsupported(failures) = generate(&doc, strict_options()).unwrap_err() else {
        panic!("expected unsupported settings")
    };
    assert_eq!(failures.settings, 1);
    assert_eq!(failures.empty_sources, 1);
    assert_eq!(
        failures.reasons["UID has no supported native representation"],
        1
    );
    assert_eq!(
        failures.reasons["nonfinite real has no supported native representation"],
        1
    );
    let summary = failures.to_string();
    assert!(summary.starts_with("1 unsupported settings; 1 unsupported empty sources."));
    assert!(summary.contains("more than one reason"));
    assert!(!summary.contains("mixed"));
    let generated = partial(&doc);
    assert_eq!(generated.unsupported, failures);
    assert_eq!(generated.settings, 1);
    assert!(generated.script.contains("'valid' -bool 'TRUE'"));
    assert!(
        !generated
            .script
            .contains(" write 'com.example.render' 'mixed'")
    );
}

#[test]
fn default_generation_preserves_supported_settings_and_skips_whole_unsupported_values() {
    let doc = document([
        ("keep", Value::Boolean(true)),
        (
            "unsupported",
            array([
                Value::String("whole setting is omitted".into()),
                Value::Uid(7),
            ]),
        ),
    ]);
    let original = doc.clone();
    let output = generate(&doc, GenerateOptions::default()).unwrap();
    assert_eq!(output, partial(&doc));
    assert_eq!(output.settings, 1);
    assert_eq!(output.unsupported.settings, 1);
    assert!(output.script.contains("'keep' -bool 'TRUE'"));
    assert!(!output.script.contains("whole setting is omitted"));
    assert_eq!(doc, original);
    assert!(matches!(
        generate(&doc, strict_options()),
        Err(Error::Unsupported(_))
    ));

    let unsupported_only = document([("unsupported", Value::Uid(7))]);
    let output = generate(&unsupported_only, GenerateOptions::default()).unwrap();
    assert_eq!(output.settings, 0);
    assert_eq!(output.unsupported.settings, 1);
    assert!(!output.script.contains("/usr/bin/defaults"));
}
