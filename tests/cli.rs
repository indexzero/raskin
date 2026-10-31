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
    assert!(
        text.contains("capture") && text.contains("generate") && text.contains("skip-unsupported")
    );
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
