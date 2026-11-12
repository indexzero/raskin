# Native command mappings

Raskin generates ordinary commands and never executes them. Installed Apple defaults(1) and pmset(1) manuals are command references. Apple's [preference guide](https://developer.apple.com/library/archive/documentation/UserExperience/Conceptual/PreferencePanes/Tasks/Preferences.html) explains scope.

| Saved data | Emitted native operation |
| --- | --- |
| User ordinary domain | /usr/bin/defaults write DOMAIN KEY TYPE VALUE |
| User current host | /usr/bin/defaults -currentHost write/import |
| System ordinary domain | sudo defaults with an explicit /Library/Preferences/DOMAIN path |
| User app container | defaults with a quoted destination HOME/Library/Containers path |
| System/container current host | Unsupported generation diagnostic |
| Boolean/integer/finite real/string/data/whole-second date | Explicit -bool/-int/-float/-string/-data/-date |
| Empty array/dictionary | -array / -dict |
| Nested string/data arrays/dictionaries | Native OpenStep property-list argument |
| Compound Boolean/numeric/date data | Native defaults import with embedded XML |
| Battery/charger/UPS | sudo pmset -b/-c/-u KEY INTEGER |

Native OpenStep property lists preserve strings, data, arrays, and dictionaries recursively. They do not retain the additional Boolean/numeric/date distinctions. See Apple's [Old-Style ASCII Property Lists](https://developer.apple.com/library/archive/documentation/Cocoa/Conceptual/PropertyLists/OldStylePlists/OldStylePLists.html).

When XML is required, a quoted here-document passes the emitted domain dictionary to defaults import. There is no companion plist file. The available manual documents importing that dictionary; it does not establish behavior for destination keys absent from the payload. Raskin adds no merge, replacement, or preservation policy.

Source UUIDs do not become target identities, but absolute paths or identifiers inside values stay literal. System writes use explicit paths; running a plain domain command through sudo is not treated as system scope.

Generation rejects UIDs, nonfinite reals, integers above signed 64-bit range, fractional dates, unrepresentable text, unsafe identities, and unsupported scopes. These can remain faithfully stored in KDL. Explicit partial generation names skipped top-level settings. It never silently drops nested members.

Tests inspect shell text and parse native data. They never execute commands. Rust plist 1.10.3's ASCII parser does not recognize native data tokens. OpenStep bytes therefore have exact text fixtures and primary-source evidence, not a claimed parser round trip. XML data and string-only OpenStep structures have parser round trips.

This evidence establishes artifacts, not successful application on a live Mac.

The writable power mappings also include tcpkeepalive, restricted to 0/1. Apple's [pmset source](https://github.com/apple-oss-distributions/PowerManagement/blob/main/pmset/pmset.m) handles it in parseArgs with checkAndSetIntValue and the Boolean constraint. That source displays lowpowermode but does not establish its writable command mapping; it remains unsupported here. Sleep On Power Button is a display label, not a guessed command argument. The installed manual describes changing networkoversleep as unsupported; it remains omitted. No hibernatefile path mapping is introduced.
