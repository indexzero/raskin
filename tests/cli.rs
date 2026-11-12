//! All child processes are the Raskin binary in fixture/generate/help/spec modes.
//! The emitted script is only read as text, never handed to a shell.
use raskin::{artifact, codec};
use std::{
    fs,
    path::Path,
    process::{Command, Output},
};

fn cli(args: &[&str], cwd: &Path) -> Output {
    Command::new(env!("CARGO_BIN_EXE_raskin"))
        .args(args)
        .current_dir(cwd)
        .output()
        .unwrap()
}
fn temp() -> tempfile::TempDir {
    tempfile::tempdir().unwrap()
}
fn input(dir: &Path) {
    fs::write(
        dir.join("input.kdl"),
        include_str!("../examples/desktop.kdl"),
    )
    .unwrap();
}
#[test]
fn generated_output_is_stable_and_never_executed() {
    let tmp = temp();
    let dir = tmp.path();
    input(dir);
    let one = cli(&["generate", "input.kdl", "-o", "one.macos"], dir);
    assert!(
        one.status.success(),
        "{}",
        String::from_utf8_lossy(&one.stderr)
    );
    assert!(one.stdout.is_empty());
    let two = cli(&["generate", "input.kdl", "-o", "two.macos"], dir);
    assert!(two.status.success());
    let one = fs::read(dir.join("one.macos")).unwrap();
    assert_eq!(one, fs::read(dir.join("two.macos")).unwrap());
    assert!(
        String::from_utf8(one)
            .unwrap()
            .contains("/usr/bin/defaults")
    );
    assert_eq!(
        fs::read_to_string(dir.join("input.kdl")).unwrap(),
        include_str!("../examples/desktop.kdl")
    );
}
#[test]
fn no_clobber_force_and_validation_before_write() {
    let tmp = temp();
    let dir = tmp.path();
    input(dir);
    fs::write(dir.join("out.macos"), "original").unwrap();
    assert!(
        !cli(&["generate", "input.kdl", "-o", "out.macos"], dir)
            .status
            .success()
    );
    assert_eq!(
        fs::read_to_string(dir.join("out.macos")).unwrap(),
        "original"
    );
    fs::write(dir.join("bad.kdl"), "raskin 999").unwrap();
    assert!(
        !cli(&["generate", "bad.kdl", "-o", "out.macos", "--force"], dir)
            .status
            .success()
    );
    assert_eq!(
        fs::read_to_string(dir.join("out.macos")).unwrap(),
        "original"
    );
    assert!(
        cli(
            &["generate", "input.kdl", "-o", "out.macos", "--force"],
            dir
        )
        .status
        .success()
    );
    assert_ne!(
        fs::read_to_string(dir.join("out.macos")).unwrap(),
        "original"
    );
    assert!(fs::read_dir(dir).unwrap().all(|e| {
        !e.unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with(".raskin-")
    }));
}
#[test]
fn aliases_and_output_errors_leave_input_intact() {
    let tmp = temp();
    let dir = tmp.path();
    input(dir);
    assert!(
        !cli(
            &["generate", "input.kdl", "-o", "./input.kdl", "--force"],
            dir
        )
        .status
        .success()
    );
    fs::hard_link(dir.join("input.kdl"), dir.join("hardlink")).unwrap();
    assert!(
        !cli(&["generate", "input.kdl", "-o", "hardlink", "--force"], dir)
            .status
            .success()
    );
    assert!(
        !cli(&["generate", "input.kdl", "-o", "absent/out.macos"], dir)
            .status
            .success()
    );
    assert!(!dir.join("absent").try_exists().unwrap());
    assert_eq!(
        fs::read_to_string(dir.join("input.kdl")).unwrap(),
        include_str!("../examples/desktop.kdl")
    );
}
#[cfg(unix)]
#[test]
fn artifacts_are_private_and_symlink_aliases_are_refused() {
    use std::os::unix::{fs::PermissionsExt, fs::symlink};
    let tmp = temp();
    let dir = tmp.path();
    input(dir);
    symlink(dir.join("input.kdl"), dir.join("alias")).unwrap();
    assert!(
        !cli(&["generate", "input.kdl", "-o", "alias", "--force"], dir)
            .status
            .success()
    );
    artifact::write(&dir.join("capture.kdl"), b"raskin 1\n", false, false).unwrap();
    assert_eq!(
        fs::metadata(dir.join("capture.kdl"))
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o600
    );
    assert!(
        cli(&["generate", "input.kdl", "-o", "output.macos"], dir)
            .status
            .success()
    );
    assert_eq!(
        fs::metadata(dir.join("output.macos"))
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o700
    );
}
#[test]
fn unknown_commands_and_short_long_help() {
    let tmp = temp();
    let dir = tmp.path();
    for command in ["apply", "diff"] {
        assert!(!cli(&[command], dir).status.success());
    }
    let short = cli(&["generate", "-h"], dir);
    let long = cli(&["generate", "--help"], dir);
    assert!(short.status.success() && long.status.success());
    assert_ne!(short.stdout, long.stdout);
    let spec = cli(&["--usage-spec"], dir);
    assert!(
        spec.status.success(),
        "{}",
        String::from_utf8_lossy(&spec.stderr)
    );
    let text = String::from_utf8(spec.stdout).unwrap();
    assert!(text.contains("capture") && text.contains("generate") && text.contains("strict"));
}
#[test]
fn offline_capture_then_generate_uses_only_the_supplied_tree() {
    let tmp = temp();
    let dir = tmp.path().canonicalize().unwrap();
    fs::create_dir(dir.join("source")).unwrap();
    fs::create_dir(dir.join("source/user")).unwrap();
    fs::write(dir.join("source/user/org.example.fixture.plist"),
        r#"<?xml version="1.0"?><plist version="1.0"><dict><key>enabled</key><true/></dict></plist>"#).unwrap();
    fs::write(
        dir.join("source/host-id.txt"),
        "AAAAAAAA-BBBB-CCCC-DDDD-EEEEEEEEEEEE\n",
    )
    .unwrap();
    fs::write(
        dir.join("source/pmset.txt"),
        "Battery Power:\n displaysleep 2\nAC Power:\n displaysleep 10\n",
    )
    .unwrap();
    let out = cli(&["capture", "--from", "source", "-o", "captured.kdl"], &dir);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let saved = fs::read_to_string(dir.join("captured.kdl")).unwrap();
    let doc = codec::decode(&saved, "captured.kdl").unwrap();
    assert_eq!(doc.domains.len(), 1);
    let generated = cli(&["generate", "captured.kdl", "-o", "captured.macos"], &dir);
    assert!(
        generated.status.success(),
        "{}",
        String::from_utf8_lossy(&generated.stderr)
    );
    let text = fs::read_to_string(dir.join("captured.macos")).unwrap();
    assert!(text.contains("org.example.fixture"));
    assert!(text.contains("-b") && text.contains("-c"));
}

fn capture_tree(dir: &Path, power: &str) {
    fs::create_dir_all(dir.join("source/user")).unwrap();
    fs::write(dir.join("source/user/org.example.fixture.plist"),
        r#"<?xml version="1.0"?><plist version="1.0"><dict><key>enabled</key><true/></dict></plist>"#).unwrap();
    fs::write(
        dir.join("source/host-id.txt"),
        "AAAAAAAA-BBBB-CCCC-DDDD-EEEEEEEEEEEE\n",
    )
    .unwrap();
    fs::write(dir.join("source/pmset.txt"), power).unwrap();
}

#[test]
fn capture_reports_partial_success_concisely_and_keeps_diagnostics_out_of_kdl() {
    let tmp = temp();
    let dir = tmp.path().canonicalize().unwrap();
    capture_tree(&dir, include_str!("fixtures/capture/pmset-labels.txt"));
    for number in 0..200 {
        fs::write(
            dir.join(format!("source/user/skipped-{number}.txt")),
            "synthetic non-preference",
        )
        .unwrap();
    }
    let normal = cli(&["capture", "--from", "source", "-o", "normal.kdl"], &dir);
    assert!(normal.status.success());
    assert!(normal.stdout.is_empty());
    let summary = String::from_utf8(normal.stderr).unwrap();
    assert!(summary.starts_with("Saved partial capture to \"normal.kdl\"."));
    assert!(summary.contains("power profiles: battery, charger"));
    assert!(summary.contains("Skipped sources/fields:") && summary.contains("--verbose"));
    assert!(summary.lines().count() <= 5, "{summary}");
    assert!(!summary.contains("skipped-199.txt"));
    let saved = fs::read_to_string(dir.join("normal.kdl")).unwrap();
    assert!(!saved.contains("unavailable") && !saved.contains("skipped-"));
    let document = codec::decode(&saved, "normal.kdl").unwrap();
    assert_eq!(document.domains.len(), 1);
    assert_eq!(document.power.len(), 2);
    assert!(document.unavailable.is_empty());

    let verbose = cli(
        &[
            "capture",
            "--from",
            "source",
            "--verbose",
            "-o",
            "verbose.kdl",
        ],
        &dir,
    );
    assert!(verbose.status.success());
    assert_eq!(saved, fs::read_to_string(dir.join("verbose.kdl")).unwrap());
    let details = String::from_utf8(verbose.stderr).unwrap();
    assert!(details.starts_with("Saved partial capture"));
    assert!(details.contains("skipped-199.txt"));
    assert!(details.contains("Sleep On Power Button"));
    assert!(details.lines().count() > 200);

    let stdout = cli(&["capture", "--from", "source"], &dir);
    assert!(stdout.status.success());
    assert_eq!(stdout.stdout, saved.as_bytes());
    assert!(
        String::from_utf8(stdout.stderr)
            .unwrap()
            .starts_with("Wrote partial capture to stdout.")
    );
}

#[test]
fn failed_capture_write_does_not_claim_that_a_capture_was_saved() {
    let tmp = temp();
    let dir = tmp.path().canonicalize().unwrap();
    capture_tree(&dir, "Battery Power:\n displaysleep 7\n");
    fs::write(dir.join("exists.kdl"), "keep me").unwrap();
    for output in ["exists.kdl", "missing/output.kdl"] {
        let result = cli(&["capture", "--from", "source", "-o", output], &dir);
        assert!(!result.status.success());
        let message = String::from_utf8(result.stderr).unwrap();
        assert!(message.starts_with("error:"), "{message}");
        assert!(!message.contains("Saved") && !message.contains("Captured"));
        assert!(result.stdout.is_empty());
    }
    assert_eq!(
        fs::read_to_string(dir.join("exists.kdl")).unwrap(),
        "keep me"
    );
}

#[test]
fn failed_power_profile_remains_visible_in_the_short_capture_summary() {
    let tmp = temp();
    let dir = tmp.path().canonicalize().unwrap();
    capture_tree(
        &dir,
        "Battery Power:\n displaysleep invalid\nAC Power:\n displaysleep 42\n",
    );
    let result = cli(&["capture", "--from", "source", "-o", "partial.kdl"], &dir);
    assert!(result.status.success());
    let message = String::from_utf8(result.stderr).unwrap();
    assert!(message.contains("Saved partial capture"));
    assert!(message.contains("power profiles: charger"));
    assert!(message.contains("Power profiles not captured: battery."));
    assert!(message.contains("read failures"));
    assert!(message.lines().count() <= 6);
    let saved = fs::read_to_string(dir.join("partial.kdl")).unwrap();
    let doc = codec::decode(&saved, "partial.kdl").unwrap();
    assert!(!doc.power.contains_key("battery"));
    assert_eq!(doc.power.len(), 1);
    assert!(doc.unavailable.is_empty());
}

/// Many nested failures in one key must not inflate its count or flood normal stderr.
fn unsupported_generation_input(dir: &Path) -> String {
    use raskin::{Document, Domain, Value};
    let date = Value::Date(plist::Date::from_xml_format("2026-01-01T00:00:00.125Z").unwrap());
    let doc = Document {
        domains: [
            (
                Domain::user("Example Cache"),
                (0..256)
                    .map(|n| (format!("repeated-{n:03}"), Value::Boolean(true).into()))
                    .collect(),
            ),
            (
                Domain::user("com.example.fixture"),
                [
                    ("keep".into(), Value::String("retained".into()).into()),
                    ("dates".into(), Value::Array(vec![date.into(); 512]).into()),
                    ("unsigned".into(), Value::Integer(u64::MAX as i128).into()),
                    ("uid".into(), Value::Uid(7).into()),
                ]
                .into(),
            ),
        ]
        .into(),
        ..Default::default()
    };
    let input = codec::encode(&doc).unwrap();
    fs::write(dir.join("unsupported.kdl"), &input).unwrap();
    input
}

#[test]
fn strict_generation_failure_summarizes_every_reason_and_expands_only_with_verbose() {
    let tmp = temp();
    let dir = tmp.path();
    let saved = unsupported_generation_input(dir);
    fs::write(dir.join("keep.macos"), "existing output").unwrap();
    for output in ["failed.macos", "keep.macos"] {
        let result = cli(
            &[
                "generate",
                "unsupported.kdl",
                "--strict",
                "-o",
                output,
                "--force",
            ],
            dir,
        );
        assert!(!result.status.success());
        assert!(result.stdout.is_empty());
        let message = String::from_utf8(result.stderr).unwrap();
        assert!(message.starts_with("error: generation stopped; no output was written."));
        assert!(message.contains("259 unsupported settings"));
        assert!(message.contains("256: unsafe domain identity"));
        assert!(message.contains("1: fractional-second date"));
        assert!(message.contains("1: integer exceeds signed 64-bit"));
        assert!(message.contains("1: UID"));
        assert!(message.contains("--verbose") && message.contains("Omit --strict"));
        assert!(!message.contains("repeated-") && !message.contains("dates\"[511]"));
        assert!(
            message.lines().count() <= 10 && message.len() < 1200,
            "{message}"
        );
    }
    assert!(!dir.join("failed.macos").exists());
    assert_eq!(
        fs::read_to_string(dir.join("keep.macos")).unwrap(),
        "existing output"
    );
    let verbose = cli(
        &[
            "generate",
            "unsupported.kdl",
            "--strict",
            "--verbose",
            "-o",
            "verbose.macos",
        ],
        dir,
    );
    assert!(!verbose.status.success());
    assert!(verbose.stdout.is_empty());
    assert!(!dir.join("verbose.macos").exists());
    let details = String::from_utf8(verbose.stderr).unwrap();
    assert!(details.starts_with("error: generation stopped; no output was written."));
    assert!(details.contains("repeated-255") && details.contains("dates\"[511]"));
    let stdout = cli(&["generate", "unsupported.kdl", "--strict"], dir);
    assert!(!stdout.status.success() && stdout.stdout.is_empty());
    assert_eq!(
        saved,
        fs::read_to_string(dir.join("unsupported.kdl")).unwrap()
    );
}

#[test]
fn default_generation_reports_saved_settings_concisely_and_keeps_stdout_clean() {
    let tmp = temp();
    let dir = tmp.path();
    unsupported_generation_input(dir);
    let result = cli(&["generate", "unsupported.kdl", "-o", "partial.macos"], dir);
    assert!(result.status.success() && result.stdout.is_empty());
    let message = String::from_utf8(result.stderr).unwrap();
    assert!(message.starts_with("Saved partial script to \"partial.macos\"."));
    assert!(message.contains("Generated 1 settings."));
    assert!(message.contains("Skipped 259 unsupported settings"));
    assert!(
        message.lines().count() <= 10 && message.len() < 1200,
        "{message}"
    );
    assert!(!message.contains("repeated-255"));
    let script = fs::read(dir.join("partial.macos")).unwrap();
    let text = String::from_utf8(script.clone()).unwrap();
    assert!(text.contains("# Partial output: 259 unsupported items skipped."));
    assert!(text.contains("'keep' -string 'retained'"));
    assert!(text.contains("repeated-255") && text.contains("dates\"[511]"));
    let stdout = cli(&["generate", "unsupported.kdl"], dir);
    assert!(stdout.status.success());
    assert_eq!(stdout.stdout, script);
    assert!(
        String::from_utf8(stdout.stderr)
            .unwrap()
            .starts_with("Wrote partial script to stdout.")
    );
    let verbose = cli(&["generate", "unsupported.kdl", "-v"], dir);
    assert!(verbose.status.success());
    assert_eq!(verbose.stdout, script);
    let details = String::from_utf8(verbose.stderr).unwrap();
    assert!(details.contains("repeated-255") && details.contains("dates\"[511]"));
}

#[test]
fn generation_write_failure_does_not_claim_success_or_dump_skip_details() {
    let tmp = temp();
    let dir = tmp.path();
    unsupported_generation_input(dir);
    fs::write(dir.join("exists.macos"), "keep me").unwrap();
    for output in ["exists.macos", "missing/output.macos"] {
        let result = cli(
            &["generate", "unsupported.kdl", "--verbose", "-o", output],
            dir,
        );
        assert!(!result.status.success() && result.stdout.is_empty());
        let message = String::from_utf8(result.stderr).unwrap();
        assert!(message.starts_with("error:"), "{message}");
        assert!(
            !message.contains("Saved")
                && !message.contains("Generated")
                && !message.contains("repeated-255")
        );
    }
    assert_eq!(
        fs::read_to_string(dir.join("exists.macos")).unwrap(),
        "keep me"
    );
}

#[test]
fn legacy_capture_records_are_summarized_during_generation() {
    let tmp = temp();
    let dir = tmp.path();
    fs::write(
        dir.join("legacy.kdl"),
        "raskin 1\nunavailable \"fixture:missing\" reason=\"missing\"\n",
    )
    .unwrap();
    let result = cli(&["generate", "legacy.kdl"], dir);
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let summary = String::from_utf8(result.stderr).unwrap();
    assert!(summary.starts_with("Wrote partial script to stdout."));
    assert!(summary.contains("Input records 1 unavailable capture sources."));
    assert!(!summary.contains("fixture:missing"));
    assert!(
        String::from_utf8(result.stdout)
            .unwrap()
            .contains("fixture:missing")
    );
}

#[test]
fn old_skip_flag_matches_the_default_and_conflicts_with_strict() {
    let tmp = temp();
    let dir = tmp.path();
    unsupported_generation_input(dir);
    let default = cli(&["generate", "unsupported.kdl"], dir);
    let legacy = cli(&["generate", "unsupported.kdl", "--skip-unsupported"], dir);
    assert!(default.status.success() && legacy.status.success());
    assert_eq!(default.stdout, legacy.stdout);
    assert_eq!(default.stderr, legacy.stderr);
    for flags in [
        ["--strict", "--skip-unsupported"],
        ["--skip-unsupported", "--strict"],
    ] {
        let result = cli(
            &[
                "generate",
                "unsupported.kdl",
                flags[0],
                flags[1],
                "-o",
                "conflict.macos",
            ],
            dir,
        );
        assert!(!result.status.success() && result.stdout.is_empty());
        assert!(!dir.join("conflict.macos").exists());
    }
    let help = cli(&["generate", "--help"], dir);
    assert!(help.status.success());
    let text = String::from_utf8(help.stdout).unwrap();
    assert!(text.contains("--strict") && !text.contains("--skip-unsupported"));
}
