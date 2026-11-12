//! Fixture-only capture tests. No test calls capture_live or a native provider.
use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::io;
use std::path::{Path, PathBuf};

use raskin::capture::{
    CaptureEntry, CaptureReader, CaptureReport, EntryKind, Preset, capture_offline,
    capture_with_reader,
};
use raskin::{Domain, Host, Reason, Scope, Value};

const UUID: &str = "ABCDEF12-3456-7890-ABCD-EF1234567890";
type Reply<T> = std::result::Result<T, io::ErrorKind>;

struct MockReader {
    directories: BTreeMap<PathBuf, Reply<Vec<CaptureEntry>>>,
    files: BTreeMap<PathBuf, Reply<Vec<u8>>>,
    host: Reply<String>,
    power: Reply<String>,
    reads: RefCell<Vec<String>>,
}
impl Default for MockReader {
    fn default() -> Self {
        Self {
            directories: ["user", "system", "containers", "group-containers"]
                .into_iter()
                .map(|p| (p.into(), Ok(Vec::new())))
                .collect(),
            files: BTreeMap::new(),
            host: Ok(UUID.into()),
            power: Ok("Battery Power:\n displaysleep 2\nAC Power:\n displaysleep 120\n".into()),
            reads: RefCell::new(Vec::new()),
        }
    }
}
impl MockReader {
    fn add_entry(&mut self, parent: &Path, entry: CaptureEntry) {
        let entries = self
            .directories
            .entry(parent.into())
            .or_insert_with(|| Ok(Vec::new()))
            .as_mut()
            .unwrap();
        if !entries.iter().any(|old| old.name == entry.name) {
            entries.push(entry);
        }
    }
    fn file(&mut self, path: impl AsRef<Path>, bytes: Reply<Vec<u8>>) {
        let path = path.as_ref();
        let mut current = path.parent().unwrap();
        while let Some(parent) = current.parent() {
            if parent.as_os_str().is_empty() {
                break;
            }
            self.add_entry(
                parent,
                CaptureEntry {
                    name: current.file_name().unwrap().into(),
                    kind: EntryKind::Directory,
                },
            );
            current = parent;
        }
        self.add_entry(
            path.parent().unwrap(),
            CaptureEntry {
                name: path.file_name().unwrap().into(),
                kind: EntryKind::File,
            },
        );
        self.files.insert(path.into(), bytes);
    }
    fn power_report(text: &str) -> CaptureReport {
        capture_with_reader(
            &Self {
                power: Ok(text.into()),
                ..Self::default()
            },
            Preset::Power,
        )
        .unwrap()
    }
}
fn reply<T: Clone>(value: &Reply<T>) -> io::Result<T> {
    value
        .clone()
        .map_err(|kind| io::Error::new(kind, "synthetic read failure"))
}
impl CaptureReader for MockReader {
    fn read_dir(&self, path: &Path) -> io::Result<Vec<CaptureEntry>> {
        self.reads
            .borrow_mut()
            .push(format!("dir:{}", path.display()));
        reply(
            self.directories
                .get(path)
                .unwrap_or(&Err(io::ErrorKind::NotFound)),
        )
    }
    fn read_file(&self, path: &Path) -> io::Result<Vec<u8>> {
        self.reads
            .borrow_mut()
            .push(format!("file:{}", path.display()));
        reply(
            self.files
                .get(path)
                .unwrap_or(&Err(io::ErrorKind::NotFound)),
        )
    }
    fn host_id(&self) -> io::Result<String> {
        self.reads.borrow_mut().push("host-id".into());
        reply(&self.host)
    }
    fn power_text(&self) -> io::Result<String> {
        self.reads.borrow_mut().push("pmset".into());
        reply(&self.power)
    }
}
fn fixture_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/capture/complete")
        .canonicalize()
        .unwrap()
}
fn bytes(dictionary: &[(&str, plist::Value)]) -> Vec<u8> {
    let value = plist::Value::Dictionary(
        dictionary
            .iter()
            .map(|(key, value)| (key.to_string(), value.clone()))
            .collect(),
    );
    let mut bytes = Vec::new();
    value.to_writer_xml(&mut bytes).unwrap();
    bytes
}
fn simple() -> Vec<u8> {
    bytes(&[("value", plist::Value::Integer(1.into()))])
}
fn omitted(report: &CaptureReport, source: &str, reason: Reason) -> bool {
    report
        .omissions
        .iter()
        .any(|entry| entry.source == source && entry.reason == reason)
}
fn diagnostic(report: &CaptureReport, text: &str) -> bool {
    report.diagnostics.iter().any(|line| line.contains(text))
}
fn domain(name: &str, scope: Scope, host: Host, container: Option<&str>) -> Domain {
    Domain {
        name: name.into(),
        scope,
        host,
        container: container.map(str::to_string),
    }
}
fn keys(report: &CaptureReport) -> BTreeSet<(Domain, String)> {
    report
        .document
        .domains
        .iter()
        .flat_map(|(domain, properties)| properties.keys().map(|key| (domain.clone(), key.clone())))
        .collect()
}
fn expected_keys(names: &[(&str, Host, &[&str])]) -> BTreeSet<(Domain, String)> {
    names
        .iter()
        .flat_map(|(name, host, keys)| {
            keys.iter()
                .map(|key| (domain(name, Scope::User, *host, None), key.to_string()))
        })
        .collect()
}

#[test]
fn presets_have_exact_stable_names() {
    for (name, expected) in [
        ("broad", Preset::Broad),
        ("desktop", Preset::Desktop),
        ("input", Preset::Input),
        ("power", Preset::Power),
    ] {
        assert_eq!(name.parse::<Preset>().unwrap(), expected);
        assert_eq!(expected.to_string(), name);
        assert_eq!(expected.as_str(), name);
    }
    assert_eq!(Preset::default(), Preset::Broad);
    assert!("Desktop".parse::<Preset>().is_err());
    assert!("all".parse::<Preset>().is_err());
}

#[test]
fn broad_preserves_xml_binary_types_and_scoped_identities() {
    let report = capture_offline(&fixture_root(), Preset::Broad).unwrap();
    let xml = &report.document.domains[&Domain::user("com.example.types")];
    let binary = &report.document.domains[&Domain::user("com.example.binary")];
    assert_eq!(xml["Boolean"].value, Value::Boolean(true));
    assert_eq!(xml["Integer"].value, Value::Integer(1));
    assert_eq!(xml["Real"].value, Value::Real(1.0));
    assert_ne!(xml["Integer"].value, xml["Real"].value);
    assert_eq!(xml["NegativeZero"].value, Value::Real(-0.0));
    assert_ne!(xml["NegativeZero"].value, Value::Real(0.0));
    assert_eq!(xml["Bytes"].value, Value::Data(vec![0, 0, 1, 15, 255]));
    assert_eq!(xml["EmptyData"].value, Value::Data(vec![]));
    assert_eq!(xml["EmptyArray"].value, Value::Array(vec![]));
    assert_eq!(xml["EmptyDictionary"].value, Value::Dict(BTreeMap::new()));
    assert_eq!(
        xml["Singleton"].value,
        Value::Array(vec![Value::String("one".into()).into()])
    );
    assert_eq!(
        xml["RepeatedArray"].value,
        Value::Array(vec![
            Value::String("same".into()).into(),
            Value::String("same".into()).into()
        ])
    );
    assert_eq!(
        xml["MaximumUnsigned"].value,
        Value::Integer(u64::MAX.into())
    );
    assert_eq!(xml["MinimumSigned"].value, Value::Integer(i64::MIN.into()));
    assert_eq!(
        xml["Date"].value,
        Value::Date(plist::Date::from_xml_format("2026-10-30T19:24:05Z").unwrap())
    );
    assert_eq!(
        binary["Date"].value,
        Value::Date(plist::Date::from_xml_format("2026-10-30T19:24:05.125Z").unwrap())
    );
    assert_eq!(binary["Uid"].value, Value::Uid(42));
    for key in xml.keys().filter(|key| key.as_str() != "Date") {
        assert_eq!(xml[key], binary[key], "type mismatch for {key}");
    }
    let Value::Dict(nested) = &xml["Nested"].value else {
        panic!("nested dictionary lost");
    };
    let Value::Array(array) = &nested["literal.dotted.key"].value else {
        panic!("literal dotted key lost");
    };
    assert_eq!(array[0].value, Value::Boolean(true));
    assert_eq!(array[1].value, Value::Integer(1));
    assert_eq!(array[2].value, Value::Real(1.0));
    assert!(matches!(array[3].value, Value::Dict(_)));
    assert!(xml.values().all(|setting| setting.metadata.is_empty()));
    for identity in [
        domain("com.apple.dock", Scope::User, Host::Current, None),
        domain("com.example.system", Scope::System, Host::Any, None),
        domain("com.example.system", Scope::System, Host::Current, None),
        domain(
            "com.example.preferences",
            Scope::User,
            Host::Any,
            Some("com.example.sandbox"),
        ),
        domain(
            "com.example.preferences",
            Scope::User,
            Host::Current,
            Some("com.example.sandbox"),
        ),
    ] {
        assert!(
            report.document.domains.contains_key(&identity),
            "missing {identity:?}"
        );
    }
    assert_eq!(
        report.document.power["battery"]["displaysleep"].value,
        Value::Integer(2)
    );
    assert_eq!(
        report.document.power["charger"]["displaysleep"].value,
        Value::Integer(120)
    );
    assert_eq!(
        report.document.power["ups"]["haltlevel"].value,
        Value::Integer(20)
    );
    assert!(omitted(
        &report,
        "defaults:system/com.apple.PowerManagement.plist",
        Reason::Unsupported
    ));
    assert!(omitted(
        &report,
        "defaults:system/SystemConfiguration",
        Reason::Unsupported
    ));
    assert!(omitted(
        &report,
        "provider:group-containers/group.example.shared",
        Reason::Unsupported
    ));
    for provider in [
        "systemsetup",
        "network",
        "registered-defaults",
        "unsaved-caches",
        "tcc",
        "managed-preferences",
    ] {
        assert!(omitted(
            &report,
            &format!("provider:{provider}"),
            Reason::Unsupported
        ));
    }
    assert!(diagnostic(&report, "preset broad: included"));
    assert_eq!(
        capture_offline(&fixture_root(), Preset::Broad).unwrap(),
        report
    );
}

#[test]
fn desktop_preset_includes_exact_keys_and_retains_current_host_scope() {
    let report = capture_offline(&fixture_root(), Preset::Desktop).unwrap();
    assert_eq!(
        keys(&report),
        expected_keys(&[
            (
                "com.apple.dock",
                Host::Any,
                &[
                    "autohide",
                    "orientation",
                    "tilesize",
                    "magnification",
                    "largesize",
                    "mineffect",
                    "show-recents",
                    "minimize-to-application",
                    "mru-spaces"
                ]
            ),
            (
                "com.apple.finder",
                Host::Any,
                &[
                    "AppleShowAllFiles",
                    "ShowPathbar",
                    "ShowStatusBar",
                    "FXPreferredViewStyle"
                ]
            ),
            (
                "com.apple.screencapture",
                Host::Any,
                &["type", "show-thumbnail", "showsClicks"]
            ),
            ("com.apple.dock", Host::Current, &["autohide"]),
        ])
    );
    assert!(report.document.power.is_empty());
    assert!(report.omissions.is_empty());
    assert!(diagnostic(&report, "included 17 keys; filtered 6 keys"));
    assert!(diagnostic(&report, "system and container stores"));
}

#[test]
fn input_preset_includes_exact_keys_without_container_or_system_values() {
    let report = capture_offline(&fixture_root(), Preset::Input).unwrap();
    assert_eq!(
        keys(&report),
        expected_keys(&[
            (
                "NSGlobalDomain",
                Host::Any,
                &[
                    "InitialKeyRepeat",
                    "KeyRepeat",
                    "AppleKeyboardUIMode",
                    "com.apple.swipescrolldirection"
                ]
            ),
            (
                "com.apple.AppleMultitouchTrackpad",
                Host::Any,
                &["Clicking", "TrackpadRightClick", "TrackpadThreeFingerDrag"]
            ),
            (
                "com.apple.driver.AppleBluetoothMultitouch.trackpad",
                Host::Any,
                &["Clicking", "TrackpadRightClick", "TrackpadThreeFingerDrag"]
            ),
            (
                "com.apple.AppleMultitouchTrackpad",
                Host::Current,
                &["Clicking", "TrackpadThreeFingerDrag"]
            ),
        ])
    );
    assert!(report.document.power.is_empty());
    assert!(report.omissions.is_empty());
    assert!(diagnostic(&report, "included 12 keys; filtered 5 keys"));
}

#[test]
fn power_preset_reads_only_power_and_presets_do_not_report_excluded_providers_failed() {
    let reader = MockReader::default();
    let report = capture_with_reader(&reader, Preset::Power).unwrap();
    assert_eq!(reader.reads.borrow().as_slice(), &["pmset"]);
    assert!(report.document.domains.is_empty());
    assert_eq!(report.document.power.len(), 2);
    assert!(report.omissions.is_empty());
    let report = capture_offline(&fixture_root(), Preset::Power).unwrap();
    assert_eq!(
        report
            .document
            .power
            .values()
            .map(BTreeMap::len)
            .sum::<usize>(),
        9
    );
    assert_eq!(
        report
            .document
            .power
            .keys()
            .map(String::as_str)
            .collect::<Vec<_>>(),
        ["battery", "charger", "ups"]
    );
    for preset in [Preset::Desktop, Preset::Input] {
        let reader = MockReader::default();
        let report = capture_with_reader(&reader, preset).unwrap();
        assert_eq!(reader.reads.borrow().as_slice(), &["dir:user"]);
        assert!(report.omissions.is_empty());
        assert!(diagnostic(&report, "exclusions:"));
    }
}

#[test]
fn missing_denied_and_malformed_sources_are_distinct_and_never_fall_back() {
    let mut reader = MockReader {
        host: Err(io::ErrorKind::PermissionDenied),
        power: Err(io::ErrorKind::NotFound),
        ..MockReader::default()
    };
    reader
        .directories
        .insert("system".into(), Err(io::ErrorKind::PermissionDenied));
    reader.file(
        "user/com.example.missing.plist",
        Err(io::ErrorKind::NotFound),
    );
    reader.file(
        "user/com.example.denied.plist",
        Err(io::ErrorKind::PermissionDenied),
    );
    reader.file(
        "user/com.example.broken.plist",
        Ok(include_bytes!("fixtures/capture/cases/malformed.xml").to_vec()),
    );
    let report = capture_with_reader(&reader, Preset::Broad).unwrap();
    for (source, reason) in [
        ("provider:host-id", Reason::PermissionDenied),
        ("provider:pmset", Reason::Missing),
        ("defaults:system", Reason::PermissionDenied),
        ("defaults:user/com.example.missing.plist", Reason::Missing),
        (
            "defaults:user/com.example.denied.plist",
            Reason::PermissionDenied,
        ),
        ("defaults:user/com.example.broken.plist", Reason::ReadFailed),
    ] {
        assert!(
            omitted(&report, source, reason),
            "missing {source} {reason:?}"
        );
    }
    assert!(report.document.domains.is_empty());
    assert!(report.document.power.is_empty());
    assert_eq!(
        reader
            .reads
            .borrow()
            .iter()
            .filter(|call| call.as_str() == "host-id")
            .count(),
        1
    );
    assert_eq!(
        reader
            .reads
            .borrow()
            .iter()
            .filter(|call| call.as_str() == "pmset")
            .count(),
        1
    );
}

#[test]
fn missing_offline_tree_and_bad_text_stay_offline() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().canonicalize().unwrap();
    let report = capture_offline(&root.join("absent"), Preset::Broad).unwrap();
    for source in [
        "defaults:user",
        "defaults:system",
        "defaults:containers",
        "provider:group-containers",
        "provider:host-id",
        "provider:pmset",
    ] {
        assert!(omitted(&report, source, Reason::Missing));
    }
    std::fs::write(root.join("pmset.txt"), [0xff, 0xfe]).unwrap();
    let report = capture_offline(&root, Preset::Power).unwrap();
    assert!(omitted(&report, "provider:pmset", Reason::ReadFailed));
    assert!(diagnostic(&report, "not UTF-8"));
}

#[test]
fn non_dictionary_and_empty_plists_do_not_invent_values() {
    let mut reader = MockReader::default();
    reader.file("user/com.example.empty.plist", Ok(bytes(&[])));
    reader.file(
        "user/com.example.array.plist",
        Ok(b"<?xml version=\"1.0\"?><plist version=\"1.0\"><array/></plist>".to_vec()),
    );
    let report = capture_with_reader(&reader, Preset::Broad).unwrap();
    assert!(report.document.domains.is_empty());
    assert!(diagnostic(&report, "empty preference dictionary"));
    assert!(
        !report
            .omissions
            .iter()
            .any(|entry| entry.source == "defaults:user/com.example.empty.plist")
    );
    assert!(omitted(
        &report,
        "defaults:user/com.example.array.plist",
        Reason::ReadFailed
    ));
}

#[test]
fn host_matching_is_exact_case_insensitive_and_unknown_hosts_are_explained() {
    let mut reader = MockReader::default();
    reader.file(
        format!(
            "user/ByHost/com.example.current.{}.plist",
            UUID.to_ascii_lowercase()
        ),
        Ok(simple()),
    );
    reader.file(
        "user/ByHost/com.example.foreign.11111111-2222-3333-4444-555555555555.plist",
        Ok(simple()),
    );
    reader.file(
        format!("user/ByHost/com.example.suffix.{UUID}extra.plist"),
        Ok(simple()),
    );
    let report = capture_with_reader(&reader, Preset::Broad).unwrap();
    assert_eq!(report.document.domains.len(), 1);
    assert!(report.document.domains.contains_key(&domain(
        "com.example.current",
        Scope::User,
        Host::Current,
        None
    )));
    assert!(diagnostic(&report, "foreign source host"));
    assert!(diagnostic(&report, "malformed source UUID suffix"));
    assert!(
        !reader
            .reads
            .borrow()
            .iter()
            .any(|call| call.contains("file:user/ByHost/com.example.foreign"))
    );
    for host in [
        Ok("not-a-uuid".to_string()),
        Err(io::ErrorKind::NotFound),
        Err(io::ErrorKind::PermissionDenied),
    ] {
        reader.host = host;
        reader.reads.borrow_mut().clear();
        let report = capture_with_reader(&reader, Preset::Broad).unwrap();
        assert!(report.document.domains.is_empty());
        assert!(diagnostic(&report, "current source host is unresolved"));
        assert!(
            !reader
                .reads
                .borrow()
                .iter()
                .any(|call| call.starts_with("file:"))
        );
    }
}

#[test]
fn duplicate_scoped_sources_reject_both_instead_of_overwriting() {
    let mut reader = MockReader::default();
    reader.file("user/.GlobalPreferences.plist", Ok(simple()));
    reader.file(
        "user/NSGlobalDomain.plist",
        Ok(bytes(&[("value", plist::Value::Integer(2.into()))])),
    );
    reader.file(
        format!("user/ByHost/com.example.same.{UUID}.plist"),
        Ok(simple()),
    );
    reader.file(
        format!(
            "user/ByHost/com.example.same.{}.plist",
            UUID.to_ascii_lowercase()
        ),
        Ok(simple()),
    );
    let report = capture_with_reader(&reader, Preset::Broad).unwrap();
    assert!(report.document.domains.is_empty());
    assert!(omitted(
        &report,
        "defaults:user/.GlobalPreferences.plist",
        Reason::ReadFailed
    ));
    assert!(omitted(
        &report,
        "defaults:user/NSGlobalDomain.plist",
        Reason::ReadFailed
    ));
    assert!(diagnostic(&report, "duplicate scoped identity"));
}

#[test]
fn duplicate_xml_and_binary_keys_are_detected_at_every_depth() {
    use plist::stream::{BinaryWriter, Event, Writer};
    let mut binary = Vec::new();
    {
        let mut writer = BinaryWriter::new(&mut binary);
        for event in [
            Event::StartDictionary(Some(2)),
            Event::String("same".into()),
            Event::Integer(1.into()),
            Event::String("same".into()),
            Event::Integer(2.into()),
            Event::EndCollection,
        ] {
            writer.write(event).unwrap();
        }
    }
    let fixtures = [
        include_bytes!("fixtures/capture/cases/duplicate.xml").to_vec(),
        include_bytes!("fixtures/capture/cases/nested-duplicate.xml").to_vec(),
        binary,
    ];
    for bytes in fixtures {
        let mut reader = MockReader::default();
        reader.file("user/com.example.duplicates.plist", Ok(bytes));
        let report = capture_with_reader(&reader, Preset::Broad).unwrap();
        assert!(report.document.domains.is_empty());
        assert!(omitted(
            &report,
            "defaults:user/com.example.duplicates.plist",
            Reason::ReadFailed
        ));
        assert!(diagnostic(&report, "duplicate plist dictionary key"));
    }
}

#[test]
fn excessive_plist_nesting_fails_before_unbounded_recursion() {
    let xml = format!(
        "<?xml version=\"1.0\"?><plist version=\"1.0\"><dict><key>deep</key>{}<string>x</string>{}</dict></plist>",
        "<array>".repeat(130),
        "</array>".repeat(130)
    );
    let mut reader = MockReader::default();
    reader.file("user/com.example.deep.plist", Ok(xml.into_bytes()));
    let report = capture_with_reader(&reader, Preset::Broad).unwrap();
    assert!(report.document.domains.is_empty());
    assert!(diagnostic(&report, "nesting limit"));
}

#[test]
fn unknown_power_profiles_and_keys_are_never_reinterpreted() {
    let report = MockReader::power_report(
        "Battery Power:\n displaysleep 2\n SleepServices 1\n hibernatefile /var/vm/sleepimage\n haltlevel 25\nAC Power:\n displaysleep 120\nUPS Power:\n haltlevel 20\nFuture Power:\n displaysleep 99\nbattery:\n sleep 88\n",
    );
    assert_eq!(
        report
            .document
            .power
            .keys()
            .map(String::as_str)
            .collect::<Vec<_>>(),
        ["battery", "charger", "ups"]
    );
    assert_eq!(report.document.power["battery"].len(), 1);
    assert!(omitted(
        &report,
        "pmset:battery/SleepServices",
        Reason::Unsupported
    ));
    assert!(omitted(
        &report,
        "pmset:battery/haltlevel",
        Reason::Unsupported
    ));
    assert!(omitted(
        &report,
        "pmset:battery/hibernatefile",
        Reason::Unsupported
    ));
    assert!(omitted(
        &report,
        "pmset:unknown:Future Power",
        Reason::Unsupported
    ));
    assert!(omitted(
        &report,
        "pmset:unknown:battery",
        Reason::Unsupported
    ));
    assert!(diagnostic(&report, "no all-profiles mapping"));
}

#[test]
fn malformed_and_duplicate_power_reject_the_affected_profile() {
    for bad in [
        " displaysleep 2\n displaysleep 3\n",
        " displaysleep 2\n sleep invalid\n",
        " displaysleep 2\n sleep 1 (extra annotation)\n",
        " displaysleep 2\nBattery Power:\n sleep 3\n",
        " displaysleep 2\n sleep 1:\n",
        "",
    ] {
        let report = MockReader::power_report(&format!(
            "Battery Power:\n{bad}AC Power:\n displaysleep 120\n"
        ));
        assert!(
            !report.document.power.contains_key("battery"),
            "accepted malformed data {bad:?}"
        );
        assert_eq!(
            report.document.power["charger"]["displaysleep"].value,
            Value::Integer(120)
        );
        assert!(
            report
                .omissions
                .iter()
                .any(|entry| entry.reason == Reason::ReadFailed)
        );
        assert!(diagnostic(&report, "rejected the complete profile"));
    }
    for malformed in ["", "displaysleep 2\n", "Battery Power\n displaysleep 2\n"] {
        let report = MockReader::power_report(malformed);
        assert!(report.document.power.is_empty());
        assert!(omitted(&report, "provider:pmset", Reason::ReadFailed));
    }
}

#[test]
fn nested_special_stores_and_group_container_absence_are_explicit() {
    let mut reader = MockReader::default();
    reader.file("user/nested/com.example.hidden.plist", Ok(simple()));
    reader.file(
        "system/PowerManagement/com.example.hidden.plist",
        Ok(simple()),
    );
    reader.file(
        "system/SystemConfiguration/com.example.hidden.plist",
        Ok(simple()),
    );
    reader.file("system/com.apple.PowerManagement.plist", Ok(simple()));
    reader.file("system/com.apple.network.eapolclient.plist", Ok(simple()));
    reader.directories.remove(Path::new("group-containers"));
    let report = capture_with_reader(&reader, Preset::Broad).unwrap();
    assert!(report.document.domains.is_empty());
    for source in [
        "defaults:user/nested",
        "defaults:system/PowerManagement",
        "defaults:system/SystemConfiguration",
        "defaults:system/com.apple.PowerManagement.plist",
        "defaults:system/com.apple.network.eapolclient.plist",
    ] {
        assert!(omitted(&report, source, Reason::Unsupported));
    }
    assert!(omitted(
        &report,
        "provider:group-containers",
        Reason::Missing
    ));
    assert!(
        !reader
            .reads
            .borrow()
            .iter()
            .any(|call| call.starts_with("file:"))
    );
    let report = capture_with_reader(&MockReader::default(), Preset::Broad).unwrap();
    assert!(diagnostic(&report, "no group containers discovered"));
}

#[test]
fn reader_paths_cannot_escape_or_convert_special_entries_into_files() {
    let mut reader = MockReader::default();
    reader.directories.insert(
        "user".into(),
        Ok(vec![
            CaptureEntry {
                name: "../elsewhere.plist".into(),
                kind: EntryKind::File,
            },
            CaptureEntry {
                name: "com.apple.dock.plist".into(),
                kind: EntryKind::Symlink,
            },
            CaptureEntry {
                name: "ByHost".into(),
                kind: EntryKind::Symlink,
            },
        ]),
    );
    let report = capture_with_reader(&reader, Preset::Desktop).unwrap();
    assert!(report.document.domains.is_empty());
    assert!(omitted(&report, "defaults:user", Reason::ReadFailed));
    assert!(omitted(
        &report,
        "defaults:user/com.apple.dock.plist",
        Reason::Unsupported
    ));
    assert!(omitted(
        &report,
        "defaults:user/ByHost",
        Reason::Unsupported
    ));
    assert_eq!(reader.reads.borrow().as_slice(), &["dir:user"]);
}

#[cfg(unix)]
#[test]
fn filesystem_symlinks_are_refused_at_file_directory_root_and_ancestor_levels() {
    use std::os::unix::fs::symlink;
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().canonicalize().unwrap();
    let real = root.join("real");
    std::fs::create_dir_all(real.join("user")).unwrap();
    std::fs::write(real.join("host-id.txt"), UUID).unwrap();
    std::fs::write(root.join("outside.plist"), simple()).unwrap();
    symlink(
        root.join("outside.plist"),
        real.join("user/com.example.link.plist"),
    )
    .unwrap();
    symlink(&real, root.join("linked-root")).unwrap();
    symlink(&real, root.join("linked-parent")).unwrap();
    symlink(root.join("outside.plist"), real.join("pmset.txt")).unwrap();
    let report = capture_offline(&real, Preset::Broad).unwrap();
    assert!(report.document.domains.is_empty());
    assert!(omitted(
        &report,
        "defaults:user/com.example.link.plist",
        Reason::Unsupported
    ));
    assert!(omitted(&report, "provider:pmset", Reason::Unsupported));
    for path in [root.join("linked-root"), root.join("linked-parent/child")] {
        let report = capture_offline(&path, Preset::Broad).unwrap();
        assert!(report.document.domains.is_empty());
        assert!(omitted(&report, "defaults:user", Reason::Unsupported));
        assert!(omitted(&report, "provider:host-id", Reason::Unsupported));
    }
    symlink(&real, real.join("user/ByHost")).unwrap();
    let report = capture_offline(&real, Preset::Input).unwrap();
    assert!(omitted(
        &report,
        "defaults:user/ByHost",
        Reason::Unsupported
    ));
}

#[test]
fn native_power_labels_do_not_discard_battery_and_charger_settings() {
    let report = MockReader::power_report(include_str!("fixtures/capture/pmset-labels.txt"));
    assert_eq!(report.document.power.len(), 2);
    assert_eq!(
        report.document.power["battery"]["displaysleep"].value,
        Value::Integer(7)
    );
    assert_eq!(
        report.document.power["charger"]["displaysleep"].value,
        Value::Integer(42)
    );
    assert_eq!(
        report.document.power["charger"]["tcpkeepalive"].value,
        Value::Integer(0)
    );
    for source in [
        "pmset:battery/Sleep On Power Button",
        "pmset:charger/Sleep On Power Button",
        "pmset:charger/Future Read Only Label",
        "pmset:battery/hibernatefile",
        "pmset:battery/networkoversleep",
        "pmset:battery/lowpowermode",
        "pmset:charger/lowpowermode",
    ] {
        assert!(omitted(&report, source, Reason::Unsupported), "{source}");
    }
    assert!(
        !report
            .omissions
            .iter()
            .any(|entry| entry.reason == Reason::ReadFailed)
    );
    assert!(report.document.unavailable.is_empty());
    let kdl = raskin::codec::encode(&report.document).unwrap();
    assert!(!kdl.contains("unavailable") && !kdl.contains("Sleep On Power Button"));
    let decoded = raskin::codec::decode(&kdl, "synthetic-power.kdl").unwrap();
    assert_eq!(decoded, report.document);
    let script = raskin::render::generate(&decoded, Default::default())
        .unwrap()
        .script;
    assert!(script.contains("/usr/bin/pmset -b 'displaysleep' '7'"));
    assert!(script.contains("/usr/bin/pmset -c 'displaysleep' '42'"));
    assert!(script.contains("/usr/bin/pmset -c 'tcpkeepalive' '0'"));
    assert!(!script.contains("Sleep On Power Button"));
    // The script is text only. Never execute it.
}

#[test]
fn duplicate_native_display_labels_still_reject_ambiguous_profiles() {
    let report = MockReader::power_report(
        "Battery Power:\n Sleep On Power Button 1\n Sleep On Power Button 0\n displaysleep 7\nAC Power:\n displaysleep 42\n",
    );
    assert!(!report.document.power.contains_key("battery"));
    assert_eq!(
        report.document.power["charger"]["displaysleep"].value,
        Value::Integer(42)
    );
    assert!(omitted(&report, "pmset:battery", Reason::ReadFailed));
    assert!(diagnostic(&report, "duplicate power key"));
}
