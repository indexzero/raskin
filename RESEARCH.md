# Raskin research

Research date: 2026-10-30. Status: source investigation complete; the user has confirmed the [intent](INTENT.md). This document records evidence and conclusions, not a settled CLI/schema specification. The confirmed intent and generation-only testing boundary govern how these sources may be used.

Raskin's intended workflow is to read a Mac's settings into KDL, generate an inspectable executable `.macos` file, store the artifacts using ordinary tools, and let human end users run the generated file on another Mac. The name honors Jef Raskin. The repository's byline is “a locus of attention centric way to read & write macos preferences.” The settings and the commands that reproduce them should occupy the user's attention; operating Raskin should take very little of it.

## Conclusions

There is useful behavior to absorb from all four primary references, but no single implementation is the right foundation unchanged. PrefWatch contributes the broadest map of macOS setting mechanisms. Mathias Bynens supplies the explicit `.macos` artifact model. Tatsh demonstrates producing a script alongside captured data. dsully demonstrates Rust-based plist handling, current-host addressing, and deliberate merge behavior. clintmod is useful evidence about the stores a backup tool needs to consider.

KDL is a good fit for the durable, editable representation. Its children and typed values can represent deeply nested preferences without XML's repeated tags or a TOML table path repeated at every level. It can also describe power profiles and other settings that are not user-default domains. This is a readability argument, not a measured claim that KDL always compresses better than another format.

The challenging work is preserving meaning: value types, user/system/current-host scope, battery versus charger, destination-specific identifiers, omissions, and the effect of an operation on existing settings. A format change alone solves none of these.

Generating a shell script makes the intended actions visible and separates generation from execution. A generator bug can still emit a wrong command, so inspectability complements semantic tests; it does not replace them. The generated file must remain understandable without reading Raskin's Rust implementation.

The user confirmed a standalone generated script, requiring no Raskin installation on the destination. Plist output is in scope only where macOS itself requires it. KDL remains the saved source. The user also clarified that Raskin generates files; native commands define their own effects when human end users run them. Raskin has no separate application policy or diff facility.

## Sources and reproducibility

All nine macOS projects discussed in the conversation were cloned into the existing ignored `research/` directory, including the two lower-priority migration tools. The official KDL specification and Rust implementation were added as supporting references. These are shallow source checkouts. No upstream installer, watcher, backup job, or settings-application command was run.

| Local directory under `research/` | Upstream | Inspected commit | License observed |
| --- | --- | --- | --- |
| `prefwatch` | [Gill0o/PrefWatch](https://github.com/Gill0o/PrefWatch) | `7736de9701cf71d0d238b6c2baca7fa3866f5064` | MIT |
| `clintmod-macprefs` | [clintmod/macprefs](https://github.com/clintmod/macprefs) | `17e77a3bf8599e9432fd943eb5c1846e8a89f498` | MIT |
| `dsully-macos-defaults` | [dsully/macos-defaults](https://github.com/dsully/macos-defaults) | `49cfaee15580ed13ea66ce7d745c5b696c410142` | MIT; source credits up-rs |
| `tatsh-macprefs` | [Tatsh/macprefs](https://github.com/Tatsh/macprefs) | `a3808eba6d0e3ce01ae65d3912a3b507ccabe725` | MIT |
| `mackup` | [lra/mackup](https://github.com/lra/mackup) | `9fc154052c2b026e4e912ef73fb6d396e4993699` | GPLv3 |
| `ibm-data-shift` | [IBM/mac-ibm-migration-tool](https://github.com/IBM/mac-ibm-migration-tool) | `488f192d7a4c326487296ab4b1a49de04404df37` | Apache-2.0 |
| `nix-darwin` | [nix-darwin/nix-darwin](https://github.com/nix-darwin/nix-darwin) | `bb0b90dbd54e207100de72b5dcbe3188c98a4b69` | MIT |
| `macos-defaults-catalog` | [yannbertrand/macos-defaults](https://github.com/yannbertrand/macos-defaults) | `ae534d931241dd8c62a813818bcb921c8c9313d8` | MIT |
| `mathiasbynens-dotfiles` | [mathiasbynens/dotfiles](https://github.com/mathiasbynens/dotfiles) | `b7c7894e7bb2de5d60bfb9a2f5e46d01a61300ea` | MIT |
| `kdl-spec` | [kdl-org/kdl](https://github.com/kdl-org/kdl) | `89c1087d5e7f530de328f18b6a0fad54ca8ea227` | CC-BY-SA-4.0 specification |
| `kdl-rs` | [kdl-org/kdl-rs](https://github.com/kdl-org/kdl-rs) | `443af89348ed63c2392f95eb1985211e77983de3` | Apache-2.0 |

Code inspection covered the capture, conversion, application, path resolution, filtering, and migration paths relevant to Raskin. It was not an audit of every line in every repository. Source links below use inspected commits. Local paths allow further investigation without trusting a search snippet or README claim.

## What “BE THE BORG” means here

The precedent is the local `../remarkable-mcp.rs` checkout at `cd807c8c0f8c0ab929fb0861ab155349f5aad02b`. Its [README](../remarkable-mcp.rs/README.md), [source teardowns](../remarkable-mcp.rs/docs/sources/), and [first ADR](../remarkable-mcp.rs/docs/adr/0001-cloud-read-first-port.md) show the method:

1. Inspect each implementation and record its strongest behavior, weaknesses, and evidence.
2. Select behavior by capability, with explicit decisions about what to take or leave.
3. Build a coherent Rust library and a thin CLI over it, keeping provenance visible.
4. Separate fixture-tested behavior from behavior actually verified on a live system.

This is not wholesale translation or the union of every feature. The reMarkable port also records where agreement between references failed to predict the live protocol. Raskin needs the same distinction between source evidence and behavior verified on actual macOS releases.

## Primary project teardowns

### PrefWatch: absorb the knowledge, replace the representation and execution structure

The inspected `prefwatch.sh` is 6,051 lines, including embedded Python. Its watcher registry distinguishes ordinary plist polling from power, printing, sharing, Bluetooth, hostnames, default applications, wallpaper, privacy state, NVRAM, time zone, security, firewall, Spotlight, Touch ID, launchd, and menu bar positioning. Its filtering and contextual notes contain much of its value.

The [power watcher](https://github.com/Gill0o/PrefWatch/blob/7736de9701cf71d0d238b6c2baca7fa3866f5064/prefwatch.sh#L5011) preserves the `Battery Power` / `AC Power` sections returned by `pmset -g custom` and emits `-b` / `-c`. Its fallback for other sections is `-a`; Raskin should explicitly model UPS and unknown profiles instead of inheriting that broad fallback.

The [command emitter](https://github.com/Gill0o/PrefWatch/blob/7736de9701cf71d0d238b6c2baca7fa3866f5064/prefwatch.sh#L2105) handles user versus system destinations, current-host flags, quoting, actual type probes, and notes about how changes take effect. These are separate concerns worth preserving in separate Rust components.

Its internal diff representation is not a lossless backup. [JSON conversion](https://github.com/Gill0o/PrefWatch/blob/7736de9701cf71d0d238b6c2baca7fa3866f5064/prefwatch.sh#L1962) replaces binary values with a length placeholder, converts dates to strings, and UIDs to integers. It documents that its JSON fallback can lose the distinction between integral floating-point values and integers. It also has heuristic type fallbacks, including interpreting `0` and `1` as booleans when a type probe is unavailable.

**Take:** setting mechanisms, scope distinctions, reasoned filtering, applicability/restart notes, and explicit explanations for detected settings that cannot be reproduced.

**Leave:** shell/Python process coordination, parsing human-oriented output as the general data model, placeholder values as backup data, and silent inference of a type or power scope.

### clintmod/macprefs: backup coverage, not a declarative `.macos` implementation

Its README is motivated by problems maintaining a hand-written `.macos`. The actual implementation is chiefly file copying using `rsync`, with separate modules for preferences, container preferences, power, startup items, shared file lists, accounts, dotfiles, and SSH files. It does not create a `.macos` manifest.

[Power handling](https://github.com/clintmod/macprefs/blob/17e77a3bf8599e9432fd943eb5c1846e8a89f498/src/system_preferences.py) selects one of two fixed PowerManagement plist locations. [Container handling](https://github.com/clintmod/macprefs/blob/17e77a3bf8599e9432fd943eb5c1846e8a89f498/src/app_store_preferences.py) gathers files and reconstructs restore destinations from plist names. Those are implementation assumptions, not evidence that every container or machine-specific file will transplant correctly.

Its utilities explain the `cfprefsd` caching problem, restart that daemon during restore, and warn-and-continue on partial rsync transfers. A “Backup Complete” message can therefore coexist with partial failures.

**Take:** the inventory of stores and the need to distinguish partial success.

**Leave:** arbitrary account/SSH/dotfile backup as an implicit part of “preferences,” whole-tree copying as the semantic model, and rebuilding identity from filenames alone.

### dsully/macos-defaults: a useful Rust reference, not merely YAML to replace

The project parses plist data into Rust values and supports explicit current-host selection, nested dictionary merges, array merging, backups, and process restarts. [Path resolution](https://github.com/dsully/macos-defaults/blob/49cfaee15580ed13ea66ce7d745c5b696c410142/src/defaults.rs#L95) considers the global domain, a same-named sandbox container, user preferences, and the destination hardware UUID for ByHost filenames.

Its application implementation directly rewrites plist files, with a fallback through `sudo tee`. A configurable process list can terminate `cfprefsd` or the owning app. Raskin can learn from these requirements without adopting a live settings mutator.

Important semantic caveats are visible in source. `!` is a special dictionary key meaning replacement, and `...` is an array merge marker that removes duplicates. These collide with potential data values and change array semantics. The [dump fallback](https://github.com/dsully/macos-defaults/blob/49cfaee15580ed13ea66ce7d745c5b696c410142/src/cmd/dump.rs) converts binary values to hex strings when YAML serialization fails, explicitly acknowledging the loss of correctness.

**Take:** typed plist handling, scoped destinations, and evidence about how different native operations affect stored values. Raskin does not adopt this project's live comparison/application workflow or its merge policy.

**Leave:** live application, in-band magic values, lossy binary conversion, and assuming a YAML-to-KDL serializer swap is enough.

### Tatsh/macprefs: the closest precedent for separate data and executable output

Its TOML is exporter configuration: filters, output behavior, and optional Git integration. It is not the saved preference document. [The exporter](https://github.com/Tatsh/macprefs/blob/a3808eba6d0e3ce01ae65d3912a3b507ccabe725/macprefs/utils.py) saves plist files and generates `exec-defaults.sh` plus `rejected-defaults.sh` during the same capture. It enumerates top-level user preference plists and the global domain, rather than all current-host, system, container, and native-command settings.

Tatsh's saved plists play the storage role proposed for KDL. Raskin's stronger separation would let a user edit saved KDL and regenerate executable output later, on a machine that does not have the original settings. Tatsh's documented workflow does not provide that separate compilation step. Its saved plist copies also retain more data than its filtered script; output artifacts do not necessarily have identical coverage.

The [converter](https://github.com/Tatsh/macprefs/blob/a3808eba6d0e3ce01ae65d3912a3b507ccabe725/macprefs/plist2defaults.py) is a useful caution. I executed only its pure conversion functions against synthetic inputs, without running the exporter or any emitted commands:

| Input | Observed result |
| --- | --- |
| Nested dictionary `{'outer': {'inner': 7}}` | No emitted command |
| Empty array | `IndexError` |
| Empty dictionary | `IndexError` |
| Bytes `01 02 0f` | Emits `-data 12f`, losing byte padding |
| Timestamp at 15:04:05 | Emits 03:04:05 using a 12-hour format without AM/PM |

These observations apply to the pinned revision and the isolated converter, not a claim about every release. They demonstrate why valid-looking shell output is insufficient evidence of faithful reproduction.

**Take:** inspectable output, deterministic ordering, configurable filtering, and an accounting of rejected values.

**Leave:** nested-value omissions, byte/date conversion behavior, exporter-managed Git commits/pushes as an assumed requirement, and filtering that cannot explain everything absent from the output.

### Mathias Bynens: preserve the executable artifact and the explanatory voice

The inspected [`.macos`](https://github.com/mathiasbynens/dotfiles/blob/b7c7894e7bb2de5d60bfb9a2f5e46d01a61300ea/.macos) is 951 lines. Its grouped, commented commands give the owner direct control over what runs. It already uses both `defaults` and PlistBuddy, alongside other native commands.

It is a curated set of Mathias's choices, not an exporter of the current user's choices. It contains settings tied to different macOS eras and broadly restarts applications at the end. Its command list is evidence to investigate, not a policy to impose on a new user.

**Take:** an ordinary executable file, plain commands, useful comments, and understandable grouping.

**Leave:** someone else's preferred values, unconditional historical tweaks, and a hand-maintained command list as the only source of truth.

## Secondary references

- **macOS-defaults catalog:** useful human descriptions, UI paths, value types, defaults/reset operations, and explicitly listed tested macOS versions. Its [Dock example](https://github.com/yannbertrand/macos-defaults/blob/ae534d931241dd8c62a813818bcb921c8c9313d8/docs/dock/autohide.md) is a good shape for setting knowledge. Its `diff.sh` captures ordinary/current-host `defaults` output and does not establish comprehensive coverage.
- **nix-darwin:** useful separation of typed setting declarations and generated activation commands. The inspected [sleep module](https://github.com/nix-darwin/nix-darwin/blob/bb0b90dbd54e207100de72b5dcbe3188c98a4b69/modules/power/sleep.nix) uses `systemsetup` and has no distinct battery/charger display-timeout options. Raskin's requested workflow should not require a Nix activation runtime on the destination.
- **Mackup:** its [application definitions](research/mackup/src/mackup/applications/macosx.cfg) are a useful location inventory. Its copy/link workflow is not the proposed settings compiler. Current documentation explains why copy mode replaces symlinked macOS preferences. Keep it as a secondary behavioral reference; the user's requested project direction excludes its central workflow.
- **IBM Data Shift:** the inspected Swift migration controller and `AppContext` implement file/app migration with inclusion rules and reports. The user does not want this transfer application model. Its separation of transferred, failed, and excluded items is still useful evidence.

## KDL: fit and limits

The [official specification](https://kdl.dev/spec/) identifies KDL 2.0.0 as stable. The [Rust parser](research/kdl-rs/README.md) preserves comments and formatting, supports source-spanned diagnostics, and represents integers as `i128` and reals as `f64`. Its inspected manifest identifies version 6.7.1 and MSRV 1.95; these are observations of the checkout, not a dependency pin or proof of the latest published crate.

KDL's nodes naturally group a domain, scope, and nested values. Explicit array/dictionary distinctions are essential: an empty collection, a single-element array, and a scalar must not become indistinguishable. Preserve array order and duplicates. Preserve boolean versus integer versus real, dates, and binary data. The spec already defines optional annotation conventions including `date-time` and `base64`; Raskin would still need to validate and interpret them. KDL annotations are not automatic type enforcement.

The [JSON-in-KDL document](research/kdl-spec/JSON-IN-KDL.md) is a useful encoding precedent, especially its treatment of ambiguous empty and single-element arrays. It is not a complete plist mapping. Archived-object UIDs, exceptional floating-point values, and missing versus explicit values need deliberate treatment. No data should be silently coerced merely to fit the chosen syntax.

KDL also permits duplicate properties with last-value semantics. Raskin needs its own rules for duplicate setting paths, conflicting scopes, schema versions, and unknown annotations. Formatting preservation and canonical generated output are separate concerns: captures should be stable in Git, while later edits should not be gratuitously rewritten.

The Rust parser's `KdlValue` equality normalizes certain exceptional floats; use domain-specific semantic comparison rather than assuming parser-node equality is sufficient for plist round-trip verification.

## Plist files versus shell scripts

| Artifact | What it preserves or communicates | What it does not decide |
| --- | --- | --- |
| `myprefs.kdl` | Editable, compact, typed settings across several macOS mechanisms | How a destination applies them |
| XML plist | Native nested preference values, including dates and binary data; readable but verbose | Correct destination, user/host scope, merge policy, or when an app notices |
| Binary plist | Native typed values in compact binary representation | Human-readable review or command intent |
| `myprefs.macos` | Visible operations, destinations, ordering, privileges, and prerequisites | Correctness of generated commands merely by existing |

Apple's installed `defaults(1)` documents XML export to stdout, binary export to a file, imports from a path or stdin, typed per-key writes, and current-host selection. It also warns that running applications may not see externally changed preferences and may overwrite them. `plutil -help` explicitly says its `-p` output is not a stable machine-readable format. Native plist parsing is preferable to interpreting that display output.

Three output approaches help explain the tradeoff. This comparison is technical background; the confirmed direction below is a standalone script, with plist output only where macOS itself requires it:

1. **Commands only.** Simple scalar preferences are excellent `defaults write` statements. Deeply nested or unusual values become harder to render correctly and inspect. Battery/charger settings remain ordinary `pmset` commands regardless of plist support.
2. **Script plus plist files.** The script names its payloads and applies them through explicit native operations. This keeps individual files readable, but the deliverable becomes a bundle. Moving only the script would be incomplete; paths, filenames, and missing-file checks become part of its contract.
3. **Script with embedded XML payloads.** A single transferred executable can contain readable, quoted here-documents for structured data. This preserves the proposed one-file destination workflow but makes the script longer. Payload handling and quoting still need tests.

A plist payload does not imply copying an entire old preference file over a new one. Native operations that replace a domain or set one compound value have different effects. The emitted commands themselves define those effects; Raskin adds no separate merge or conflict-resolution policy. Generation must faithfully represent the captured data. Documentation and source evidence must support the chosen command mappings; agents must not validate them by modifying actual preferences. Editing a file and using a native preferences command are distinct operations.

The confirmed direction keeps KDL authoritative and the `.macos` handoff standalone. Generate native plist output only where macOS requires it, rather than choosing plists merely for convenience with nested values. If a native operation needs a plist file, its handling belongs in the generated workflow. The exact native command mappings remain design work.

## Coverage and portability

The original battery-timeout failure is a useful acceptance case. The dotfiles snapshot already captured `pmset -g custom`, but its display diff could discard the profile headings. `install/macos` left power changes commented out. The pipeline lost the setting between observation and application; an apparently successful export was not enough.

A value should retain enough context to answer: what was read, from which store and scope, whether it was included, what operation reproduces it, and what evidence supports that operation. Reading explicit stored preferences is also different from knowing an application's effective defaults; installed applications and macOS versions can supply defaults that were never written to a plist.

Distinguish user domains, global domains, current-host values, system stores, sandbox containers, and dedicated native APIs/commands. Resolve target machine identifiers deliberately. A source UUID in a filename, a device-specific display identifier inside a value, an absolute home path, and a network service name are different portability problems.

The confirmed default is broad capture, with 2–3 common presets for curated portable subsets. Unsupported, filtered, permission-denied, missing, and failed are different outcomes that must remain visible. Any coverage statement must describe its denominator; “all settings” cannot be inferred from a successful `defaults read`. What can be captured and what can be faithfully expressed as native commands may differ; generation must account for that difference.

## Rust CLI preferences applied to this project

All ten Markdown files currently under `../0x/slop/debate/rust-cli-aspects/` were read. They include daemon and TUI guidance as well as CLI guidance; those runtime/UI requirements do not automatically apply to this one-shot CLI.

The applicable preferences are:

- A reusable library holds the domain model, capture logic, validation, and rendering. The CLI remains a thin wrapper with typed library errors and useful diagnostics.
- **jdx/usage is mandated.** The [V2 worklog](../0x/slop/debate/rust-cli-aspects/WORKLOG-V2.md) explicitly corrects the earlier failure to recognize it as a named tool. Commit or emit the usage specification and generate completions, manpages, and command reference through it. `clap` can remain the argument parser; `clap_complete` and `clap_mangen` are not the requested documentation approach.
- Keep `raskin.usage.kdl` (the CLI interface) conceptually separate from `myprefs.kdl` (the captured settings). Sharing a language does not make them the same schema.
- Use mise for a consistent development toolchain; use hk for project hooks; follow dist/cargo-binstall conventions for releases. Declare and test the MSRV, commit the lockfile, and keep dependency features narrow.
- Test capture/generation CLI output and examples using fixtures, without executing generated scripts or modifying macOS settings. Keep machine-readable stdout clean and diagnostics separate. Use actionable, source-located errors for malformed documents.
- Write durable artifacts atomically; version their schema; make paths explicit or configurable; avoid surprise background jobs and implicit state.
- KDL is the user's explicit choice for the preferences document and overrides the older generic TOML preference. Cargo/mise manifests remain their native formats.
- Tokio, SQLite, a daemon supervisor, a TUI, Node bindings, and non-macOS capture targets are not justified merely because the broader corpus describes them. They require a project need.

## What generation-only testing can establish

This research ran isolated synthetic converter checks and read native command help. It did not test Raskin, which does not exist yet, execute generated shell output, change macOS settings, or perform a live migration.

The user explicitly requires that agents and tests **never execute generated `.macos` shell files anywhere**. There is no exception for CI, a VM, another Mac, a disposable account, or a disposable preference domain. Do not execute emitted commands individually or reproduce their effects through another API. Testing generates and inspects files using fixtures; it never changes actual settings or restarts apps or preference services.

Within that boundary, meaningful evidence includes fixture data passing through capture adapters, KDL serialization/parsing, and command rendering without losing type or scope. Fixtures should include nested dictionaries, empty and single-element collections, duplicate array items, byte values with leading zeroes, dates after noon, integers versus booleans, integral reals, Unicode, quotes, shell metacharacters, and battery/charger separation. Inspect generated file contents without executing them.

These tests establish properties of the representation and generated files. They do not establish that a live Mac adopted the settings. Source evidence, fixture coverage, and any observations independently supplied by human end users must remain clearly distinguished. The reMarkable project's distinction between evidence levels is useful; its live verification process does not authorize live settings tests here.

Human end users decide whether and when to execute generated files themselves.

## File comparison follows from the artifacts

Generate a fresh KDL file on the new machine and compare it with the existing KDL file using `diff`, Git, or another ordinary comparison tool. Alternatively, generate a fresh `.macos` file from that capture and compare it with the existing `.macos` file. Compare matching formats and capture scopes or presets.

Readable, consistently structured files make this possible without a Raskin diff facility. The [README](README.md) documents this workflow. This is another consequence of keeping the files and commands at the center of the user's attention.

## Confirmed interview conclusions

The user confirmed the [intent](INTENT.md): broad capture by default; 2–3 curated portable presets; KDL as the saved source; standalone generated `.macos` output; plist output only where macOS requires it; native commands defining their own effects when users run them; ordinary tools for backup, transfer, and diffing; and generation-only agent testing with no settings changes anywhere.

The interview does not settle command names, schema details, exact preset contents, or command mappings. Those are subsequent design work, not missing product intent. Agent instructions in [AGENTS.md](AGENTS.md) preserve the execution boundary for future work.
