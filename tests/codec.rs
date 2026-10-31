use raskin::{
    Domain, Host, Scope, Value,
    codec::{decode, encode},
};

fn roundtrip(input: &str) {
    let parsed = decode(input, "fixture.kdl").unwrap();
    let encoded = encode(&parsed).unwrap();
    assert_eq!(parsed, decode(&encoded, "canonical.kdl").unwrap());
    assert_eq!(
        encoded,
        encode(&decode(&encoded, "canonical.kdl").unwrap()).unwrap()
    );
}
#[test]
fn approved_synthetic_examples_roundtrip() {
    for input in [
        include_str!("fixtures/schema/minimal.kdl"),
        include_str!("fixtures/schema/nested-values.kdl"),
        include_str!("fixtures/schema/scopes-and-omissions.kdl"),
    ] {
        roundtrip(input);
    }
}
#[test]
fn hierarchy_scopes_and_literal_dotted_keys() {
    let input = r#"raskin 1
defaults {
    org scope="system" {
        enabled #true
        example {
            app { com.apple.literal "text"; }
            app host="current" { enabled #false; }
            app container="org.example.container" { enabled #true; }
        }
    }
}"#;
    let doc = decode(input, "scope.kdl").unwrap();
    assert_eq!(doc.domains.len(), 4);
    let root = Domain {
        scope: Scope::System,
        ..Domain::user("org")
    };
    assert!(doc.domains[&root].contains_key("enabled"));
    assert!(doc.domains[&Domain::user("org.example.app")].contains_key("com.apple.literal"));
    let host = Domain {
        host: Host::Current,
        ..Domain::user("org.example.app")
    };
    assert_eq!(doc.domains[&host]["enabled"].value, Value::Boolean(false));
    roundtrip(input);
    let output = encode(&doc).unwrap();
    assert_eq!(output.matches("example {").count(), 1);
    assert!(!output.contains("defaults \"org"));
}
#[test]
fn every_type_and_collection_shape() {
    roundtrip(
        r#"raskin 1
defaults { org { example { types {
    yes #true source="fixture"
    no #false
    min -9223372036854775808
    max 18446744073709551615
    integer 0
    real 0.0
    negative-zero -0.0
    "nan" #nan
    positive-infinity #inf
    negative-infinity #-inf
    bytes (base64)"AAEP"
    reference (plist-uid)18446744073709551615
    date (date-time)"2026-10-30T15:04:05.125Z"
    text "a\"b\\c\n$HOME \u{60}quoted\u{60} café \u{0}"
    (array)empty {}
    (dict)emptyDict {}
    (array)singleton "only"
    (array)duplicates "x" "x"
    (array)mixed {
        - "text" origin="test"
        (dict)- {
            "-" 1
            (array)list 1 1.0 #true (base64)"AA=="
        }
        (array)- { (dict)- {}; }
    }
} } } }
"#,
    );
}
#[test]
fn floating_equality_distinguishes_special_values() {
    assert_ne!(Value::Real(0.0), Value::Real(-0.0));
    assert_ne!(Value::Real(f64::NAN), Value::Real(0.0));
    assert_ne!(Value::Real(f64::INFINITY), Value::Real(f64::MAX));
    assert_eq!(Value::Real(f64::NAN), Value::Real(f64::NAN));
}
#[test]
fn malformed_inputs_are_located_and_rejected() {
    let invalid = [
        ("defaults {}", "missing"),
        ("raskin 2", "version"),
        ("raskin 1\nraskin 1", "duplicate"),
        ("raskin 1\nunknown {}", "unknown"),
        ("raskin 1\nraskin-extra {}", "unknown"),
        (
            "raskin 1\npmset { battery { v 1; }; battery { v 2; }; }",
            "duplicate",
        ),
        ("raskin 1\ndefaults { app { v 1; v 2; }; }", "duplicate"),
        (
            "raskin 1\ndefaults { app { v 1; }; app { x 2; }; }",
            "duplicate",
        ),
        (
            "raskin 1\ndefaults { app host=\"any\" host=\"current\" { v 1; }; }",
            "duplicate",
        ),
        (
            "raskin 1\ndefaults { app host=\"current\" { nested { v 1; }; }; }",
            "scope",
        ),
        ("raskin 1\ndefaults { app { v #null; }; }", "null"),
        ("raskin 1\ndefaults { app { v 1 2; }; }", "exactly one"),
        ("raskin 1\ndefaults { app { v 1 {}; }; }", "no children"),
        (
            "raskin 1\ndefaults { app { v 18446744073709551616; }; }",
            "range",
        ),
        (
            "raskin 1\ndefaults { app { v (plist-uid)-1; }; }",
            "unsigned",
        ),
        ("raskin 1\ndefaults { app { v (base64)\"!\"; }; }", "base64"),
        (
            "raskin 1\ndefaults { app { v (date-time)\"yesterday\"; }; }",
            "date",
        ),
        (
            "raskin 1\ndefaults { app { v (mystery)1; }; }",
            "annotation",
        ),
        (
            "raskin 1\ndefaults { app { (mystery)v {}; }; }",
            "annotation",
        ),
        ("raskin 1\ndefaults { app { (array)v; }; }", "explicit"),
        ("raskin 1\ndefaults { app { (dict)v; }; }", "children"),
        (
            "raskin 1\ndefaults { app { (array)v 1 { - 2; }; }; }",
            "mix",
        ),
        (
            "raskin 1\ndefaults { app { (array)v { wrong 2; }; }; }",
            "named -",
        ),
        (
            "raskin 1\ndefaults { app { (dict)v { x 1; x 2; }; }; }",
            "duplicate",
        ),
        (
            "raskin 1\ndefaults { app { v 1 inline=x inline=y; }; }",
            "duplicate",
        ),
        ("raskin 1\nunavailable \"x\" reason=\"filtered\"", "reason"),
        ("raskin 1\npmset { battery { (array)key 1; }; }", "scalar"),
    ];
    for (input, needle) in invalid {
        let error = decode(input, "bad.kdl").unwrap_err().to_string();
        assert!(error.starts_with("bad.kdl:"), "{input}: {error}");
        assert!(error.contains(needle), "{input}: {error}");
    }
}
#[test]
fn parser_limits_ignore_strings_and_comments() {
    let nested = format!(
        "raskin 1\ndefaults {{ {}v 1;{} }}",
        "x {".repeat(130),
        "}".repeat(130)
    );
    assert!(
        decode(&nested, "deep.kdl")
            .unwrap_err()
            .to_string()
            .contains("nesting limit")
    );
    let braces = "{".repeat(256);
    roundtrip(&format!(
        "raskin 1\ndefaults {{ app {{ text \"{braces}\"; raw #\"{braces}\"#; }} }} // {braces}"
    ));
    assert!(
        decode("raskin 1\ndefaults { app { text \"oops", "syntax.kdl")
            .unwrap_err()
            .to_string()
            .starts_with("syntax.kdl:")
    );
}

#[test]
fn every_kdl_line_ending_terminates_comments_before_nesting_checks() {
    for newline in [
        "\r\n", "\r", "\n", "\u{0085}", "\u{000b}", "\u{000c}", "\u{2028}", "\u{2029}",
    ] {
        let prefix = format!("raskin 1\n// comment café {{{newline}");
        let shallow = format!("{prefix}defaults {{ app {{ enabled #true; }} }}");
        let document = decode(&shallow, "shallow.kdl").unwrap();
        assert_eq!(
            document.domains[&Domain::user("app")]["enabled"].value,
            Value::Boolean(true),
            "newline {newline:?}"
        );

        let deep = format!(
            "{prefix}defaults {{ {}key 1;{} }}",
            "x {".repeat(130),
            "};".repeat(130)
        );
        let error = decode(&deep, "deep.kdl").unwrap_err().to_string();
        assert!(
            error.contains("nesting limit"),
            "newline {newline:?}: {error}"
        );
    }
}
