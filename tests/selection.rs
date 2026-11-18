//! Selector parsing is pure. These tests never read preferences or run scripts.
use raskin::{Domain, Host, Scope, selection::FilePreset};

fn decode(body: &str) -> FilePreset {
    FilePreset::decode(&format!("raskin-preset 1\n{body}"), "preset.kdl").unwrap()
}

#[test]
fn exact_keys_scopes_and_collection_names_have_no_values() {
    let preset = decode(
        r#"
defaults {
    NSGlobalDomain { AppleInterfaceStyle; }
    com {
        apple {
            dock { autohide; persistent-apps; "literal.dotted.key"; }
            controlcenter host="current" { Battery; }
            finder scope="system" { ShowPathbar; }
            Terminal container="com.apple.Terminal" { "Window Settings"; }
        }
    }
}
pmset {
    battery { displaysleep; }
    charger { displaysleep; }
    ups { sleep; }
}
"#,
    );
    assert_eq!(preset.domains().len(), 5);
    let keys = &preset.domains()[&Domain::user("com.apple.dock")];
    assert_eq!(
        keys.iter().map(String::as_str).collect::<Vec<_>>(),
        ["autohide", "literal.dotted.key", "persistent-apps"]
    );
    assert!(
        preset
            .domains()
            .keys()
            .any(|domain| domain.host == Host::Current)
    );
    assert!(
        preset
            .domains()
            .keys()
            .any(|domain| domain.scope == Scope::System)
    );
    assert!(
        preset
            .domains()
            .keys()
            .any(|domain| domain.container.as_deref() == Some("com.apple.Terminal"))
    );
    assert_eq!(preset.power().len(), 3);
}

#[test]
fn namespace_scope_is_local_and_empty_groups_select_nothing() {
    let preset = decode(
        r#"
defaults {
    com {
        example host="current" {
            direct
            child { leaf; }
        }
    }
    com { example { other; } }
    empty {}
}
pmset { battery {} }
"#,
    );
    assert_eq!(preset.domains().len(), 3);
    assert!(
        preset
            .domains()
            .contains_key(&Domain::user("com.example.child"))
    );
    assert!(preset.domains().contains_key(&Domain::user("com.example")));
    assert!(preset.power().is_empty());
    let empty = decode("defaults { com { apple {} } }\npmset {}\n");
    assert!(empty.domains().is_empty() && empty.power().is_empty());
}

#[test]
fn invalid_selectors_are_located_and_never_reinterpreted_as_values() {
    for input in [
        "",
        "raskin 1",
        "raskin-preset 2",
        "raskin-preset 1 2",
        "raskin-preset 1 x=1",
        "raskin-preset 1 {}",
        "(tag)raskin-preset 1",
        "raskin-preset (tag)1",
        "raskin-preset 1\nraskin-preset 1",
        "raskin-preset 1\ninclude desktop",
        "raskin-preset 1\nunavailable foo reason=missing",
        "raskin-preset 1\ndefaults {}\ndefaults {}",
        "raskin-preset 1\ndefaults {}\npmset {}\npmset {}",
        "raskin-preset 1\ndefaults { com { apple { dock { autohide #true; } } } }",
        "raskin-preset 1\ndefaults { com { apple { dock { autohide inline=meta; } } } }",
        "raskin-preset 1\ndefaults { com { apple { dock { (array)tiles {} } } } }",
        "raskin-preset 1\ndefaults { com { apple { dock { (tag)autohide; } } } }",
        "raskin-preset 1\ndefaults { com host=current { apple { dock { a; } } } }",
        "raskin-preset 1\ndefaults { com { apple { dock host=other { a; } } } }",
        "raskin-preset 1\ndefaults { com { apple { dock host=1 { a; } } } }",
        "raskin-preset 1\ndefaults { com { apple { dock host=(tag)current { a; } } } }",
        "raskin-preset 1\ndefaults { com { apple { dock scope=other { a; } } } }",
        "raskin-preset 1\ndefaults { com { apple { dock include=foo { a; } } } }",
        "raskin-preset 1\ndefaults { com { apple { dock host=any host=current { a; } } } }",
        "raskin-preset 1\ndefaults { com { apple { dock { a; a; } } } }",
        "raskin-preset 1\ndefaults { com.apple.dock { a; }; com { apple { dock { b; } } } }",
        "raskin-preset 1\ndefaults { \"com..apple\" { a; } }",
        "raskin-preset 1\npmset { battery { displaysleep 2; } }",
        "raskin-preset 1\npmset { battery { displaysleep {} } }",
        "raskin-preset 1\npmset { battery {}; battery { sleep; } }",
        "raskin-preset 1\npmset { battery { sleep; sleep; } }",
    ] {
        let error = FilePreset::decode(input, "bad-preset.kdl")
            .unwrap_err()
            .to_string();
        assert!(error.starts_with("bad-preset.kdl:"), "{input:?}: {error}");
    }
}

#[test]
fn preset_parser_shares_bounded_depth_and_input_checks() {
    let deep = format!(
        "raskin-preset 1\ndefaults {{ {} {} }}",
        "n {".repeat(200),
        "}".repeat(200)
    );
    assert!(
        FilePreset::decode(&deep, "deep.kdl")
            .unwrap_err()
            .to_string()
            .contains("nesting limit")
    );
    let deep_comment = format!("raskin-preset 1\n{}{}", "/*".repeat(200), "*/".repeat(200));
    assert!(FilePreset::decode(&deep_comment, "comment.kdl").is_err());
    let oversized = " ".repeat(raskin::model::MAX_DOCUMENT_BYTES + 1);
    assert!(
        FilePreset::decode(&oversized, "large.kdl")
            .unwrap_err()
            .to_string()
            .contains("128 MiB")
    );
    for newline in [
        '\n', '\r', '\u{000b}', '\u{000c}', '\u{0085}', '\u{2028}', '\u{2029}',
    ] {
        let input = format!("// comment{newline}{deep}");
        assert!(
            FilePreset::decode(&input, "newline.kdl")
                .unwrap_err()
                .to_string()
                .contains("nesting limit")
        );
    }
    let key = "{".repeat(200);
    let input = format!("raskin-preset 1\ndefaults {{ com.example {{ \"{key}\"; }} }}");
    assert!(FilePreset::decode(&input, "quoted.kdl").is_ok());
}

#[test]
fn shipped_key_only_example_is_valid() {
    let preset = FilePreset::decode(
        include_str!("../examples/presets/selected.kdl"),
        "selected.kdl",
    )
    .unwrap();
    assert_eq!(preset.domains().len(), 4);
    assert_eq!(preset.power().len(), 2);
}
