# Capture coverage

Raskin captures explicit persisted settings, not every effective value or everything visible in System Settings. It never flushes caches or restarts apps. Source evidence: Apple defaults(1)/pmset(1), and the pinned implementations in [RESEARCH.md](../RESEARCH.md).

| Mechanism | Initial capture | Generation | Limit |
| --- | --- | --- | --- |
| User/global preference files | Typed XML/binary read | defaults write/import | Persisted state may lag a running app |
| Current-host user files | Current source UUID only | defaults -currentHost | Source UUID never becomes target UUID |
| System preference files | Typed read; permission-dependent | Explicit /Library path and sudo | Specialized stores have separate owners |
| System ByHost | Omitted; no supported writable mapping | Unsupported diagnostic for older KDL | No guessed destination path |
| Sandboxed app containers | Supported any-host settings; permission-dependent | Explicit container preference path | Current-host container settings omitted |
| Group containers | Unsupported observation | Unsupported | Schema has no group-container store selector |
| Nested/special preference directories | Discovered, unsupported if no scope mapping | Unsupported | No guessed basename domain |
| Battery/charger/UPS | pmset -g custom, distinct profiles | Native -b/-c/-u for documented keys | Hardware/OS capabilities differ |
| systemsetup (time zone, startup, remote services) | Unsupported provider record | Unsupported | Not part of approved executable family |
| Network, DNS, proxy, locations, VPN, Wi-Fi | Unsupported provider record | Unsupported | Service/device identities require separate mapping |
| TCC/privacy, MDM/managed settings | Unsupported provider records | Unsupported | Permissions/policy are not portable defaults |
| NVRAM, boot, account/security/FileVault settings | Unsupported provider records | Unsupported | Outside defaults and pmset mapping |
| Sharing, firewall, printer/CUPS, Time Machine/exclusions | Unsupported provider records | Unsupported | Dedicated APIs/commands |
| Wallpaper, displays, spaces, menus, accessibility databases | Ordinary persisted preferences if present; dedicated stores unsupported | Native preference mappings only | Some values contain machine-local identifiers |
| Login/background items, launch services, browser databases | Ordinary persisted preferences if present; dedicated stores unsupported | Native preference mappings only | Database/registration APIs not covered |
| Application registered defaults and unsaved caches | Unsupported provider record | No invented values | Absence of a stored key is not its effective value |

Broad capture enumerates supported locations without a curated key filter and reports omitted sources on stderr. Only settings accepted by the shared generation support checks enter KDL. Unsupported types, names/scopes, XML-incompatible values, and out-of-range power values are omitted with capture diagnostics. A nested failure excludes its whole top-level setting; emptied domains and profiles are omitted. Apple is the default union of 15 system categories plus Terminal. Exact domains and overlapping global preferences are documented in [presets](presets.md) and fixture-tested. Other standalone Apple apps and third-party apps are available through broad or an explicit file-defined preset. Absolute home paths inside values are retained literally. Unsupported power display labels (including Sleep On Power Button) are skipped individually, so they do not discard neighboring battery/charger settings. Genuine malformed values and duplicate data reject the affected profile with an explicit diagnostic. Unknown profiles never become all-profile commands.

Generation emits supported settings by default, skipping complete top-level settings with unsupported values or scopes and naming each skip. `--strict` explicitly requests failure before output if any unsupported item is found. Nested string/data collections use native OpenStep plist arguments. Compound Boolean/numeric/date values use native XML input embedded in the standalone script. The emitted `defaults import` command defines its behavior; no additional merge, replacement, or preservation guarantee is provided. No live application result is claimed by the fixture suite.

File-defined presets select exact scoped keys before generation-support checks. Unselected stores and preference files are not opened. Power selection ignores unselected profiles and fields. Malformed or duplicate selected power data still rejects its profile. Missing selected fields remain outside captured KDL and appear in the diagnostic summary. See [the preset schema](preset-schema.md).
