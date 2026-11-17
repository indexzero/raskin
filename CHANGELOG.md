# Changelog

## 0.1.0 (unreleased)

- Add file-defined presets through `capture -p FILE`, selecting exact scoped keys and separate power profiles. Keep all existing selections as built-in presets.

- Default capture to apple: the union of 15 system categories plus Terminal. Keep broad available explicitly.
- Retain all supported fields in category domains, including unknown fields and counters. Desktop and input now select whole domains and supported system/container scopes.
- Ignore new local .kdl and .macos files while retaining tracked fixtures and interface artifacts.

- Capture typed hierarchical KDL from persisted preference stores and separate power profiles.
- Add capture diagnostics and desktop, input, and power presets beside broad capture.
- Generate standalone native command files with required embedded XML payloads.
- Add atomic private artifacts, fixture verification, usage documentation, mise/hk, and dist/binstall configuration.

- Keep unavailable capture data out of saved KDL; retain compatibility when reading earlier files.
- Print concise capture results after a successful write; add --verbose for per-source details.
- Preserve battery/charger settings beside native multiword display labels; support the TCP keepalive Boolean setting.
