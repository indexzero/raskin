# raskin

a locus of attention centric way to read &amp; write macos preferences

Raskin reads persisted macOS preferences into editable KDL and generates an ordinary .macos file. Named for Jef Raskin, it keeps the settings and their native commands at the center of your attention.

## Install from this checkout

This initial implementation has not been published to a registry or release page. With Rust 1.96 or later:

~~~sh
mise exec rust@1.96.0 -- cargo install --path . --locked
~~~

Or use your existing Rust toolchain:

~~~sh
cargo install --path . --locked
~~~

Live capture targets macOS 13 or later. Offline capture and generation also work on other platforms. The generated file uses macOS commands and needs no Raskin installation on its destination.

## Examples

Capture the default Apple selection, then generate:

~~~sh
raskin capture -o myprefs.kdl
raskin generate myprefs.kdl -o myprefs.macos
~~~

Capture only the keys you care about with a [file-defined preset](docs/preset-schema.md).
Start from [the key-only example](examples/presets/selected.kdl), remove unwanted keys, and add your own:

~~~sh
raskin capture -p set-of-values-that-i-care-about.kdl -o actuallymatter.kdl
raskin generate actuallymatter.kdl -o actuallymatter.macos
~~~

That preset lists keys without values. Capture reads their current stored values, retaining complete arrays and dictionaries.
Missing or unsupported values are omitted. The file is the complete selection, with no implicit Apple additions.

The existing selections remain **built-in presets**. Select a category or the wider capture:

~~~sh
raskin capture --preset desktop -o desktop.kdl
raskin capture --preset input -o input.kdl
raskin capture --preset power -o power.kdl
raskin capture --preset terminal -o terminal.kdl
raskin capture --preset broad -o broad.kdl
~~~

[Presets](docs/presets.md) define the exact selections. Apple is the default: all system category presets plus Terminal. Other standalone Apple apps and third-party apps require broad. Categories retain supported unknown fields and can overlap, especially through global preferences.

KDL contains only settings that Raskin can generate. Capture and generation share support checks; unavailable or unsupported settings are omitted, like undefined object fields in JSON. If a nested value is unsupported, its complete top-level setting is omitted. Capture reports the saved file, captured counts, power profiles, and a compact summary of skipped sources on stderr. Add --verbose for per-source diagnostics.

Generate from the synthetic example without reading this Mac:

~~~sh
raskin generate examples/desktop.kdl -o example.macos
~~~

The output contains commands such as:

~~~sh
/usr/bin/defaults write 'com.apple.dock' 'autohide' -bool 'true'
sudo /usr/bin/pmset -b 'displaysleep' '5'
sudo /usr/bin/pmset -c 'displaysleep' '30'
~~~

Generation emits supported settings by default. Unsupported settings are skipped as complete top-level values and named in script comments. Add --verbose for per-setting details on stderr.

Use --strict if unsupported settings should stop generation before any output:

~~~sh
raskin generate myprefs.kdl --strict -o myprefs.macos
~~~

After a successful write, generation reports the destination and number of generated settings. Partial output also summarizes skipped settings by reason; --verbose expands the details on stderr. Counts refer to top-level settings, including when a nested value is unsupported.

Paths supplied with -o are written atomically. Existing output is refused unless --force is supplied. KDL files are private; generated scripts are private and executable. Generation refuses to overwrite its KDL input. Capture also refuses to overwrite its preset file, including through an alias.

## The file is the operation

Back up and transfer the files with your existing tools. Inspect the generated script and then run it yourself on the destination Mac:

~~~sh
./myprefs.macos
~~~

**That last command is for human end users. Agents and tests never execute generated scripts anywhere.**

Raskin's job ends at generation. Native commands define what running the file does. It adds no apply, merge, conflict-resolution, or replay facility.

Simple values use typed defaults commands. Nested string/data collections use native OpenStep arguments. Compound Boolean/numeric/date types need native XML input, embedded for defaults import. There are no companion plist files. That command imports the emitted domain dictionary; Raskin makes no additional promise about unlisted destination keys. See [native mappings](docs/native-commands.md).

## Diffing

Capture a new KDL file on the new machine using the same scope or preset. Compare it with the existing KDL file. Alternatively, regenerate both .macos files and compare those:

~~~sh
diff -u old-machine.kdl new-machine.kdl
diff -u old-machine.macos new-machine.macos
~~~

Stable ordering makes ordinary diff tools and Git useful. Raskin does not need a diff command.

## Coverage and limits

Capture reads explicit persisted preferences, including user/global domains, current-host files, system files, app containers, and separate power profiles. It does not flush application caches or discover every effective default. Captures can contain private application state and machine-specific values.

[Coverage](docs/coverage.md) lists supported and unsupported mechanisms. New captures omit UIDs, fractional dates, exceptional reals, unsafe identities, unsupported scopes, and other settings without a supported native mapping. Existing or edited KDL containing those values remains readable and generation reports its skips. Absolute paths inside values stay literal.

See the [user guide](docs/user-guide.md), [schema](docs/schema.md), and generated [command reference](docs/command-reference.md). The [usage specification](raskin.usage.kdl) is separate from preference KDL.

## Development

See [CONTRIBUTING.md](CONTRIBUTING.md). Tests use synthetic data, generate files, and inspect structure or text. They never run emitted commands or modify actual settings, including in CI or disposable environments.

Fixture verification establishes properties of the files, not that a live Mac adopted the settings.

## License

[MIT](LICENSE).
