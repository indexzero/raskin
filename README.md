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

Capture broadly, then generate:

~~~sh
raskin capture -o myprefs.kdl
raskin generate myprefs.kdl -o myprefs.macos
~~~

Select a curated subset:

~~~sh
raskin capture --preset desktop -o desktop.kdl
raskin capture --preset input -o input.kdl
raskin capture --preset power -o power.kdl
~~~

[Presets](docs/presets.md) define the exact selections. Broad capture is the default. KDL contains only captured settings. Unavailable values are omitted, like undefined object fields in JSON. Capture reports the saved file, captured counts, power profiles, and a compact summary of skipped sources on stderr. Add --verbose for per-source diagnostics.

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

If retained settings have no supported mapping, generation fails before writing an artifact and prints a short count by reason. Add --verbose for per-setting details. To explicitly generate a partial file with every skipped setting named in script comments:

~~~sh
raskin generate myprefs.kdl --skip-unsupported -o partial.macos
~~~

After a successful write, generation reports the destination and number of generated settings. Partial output also summarizes skipped settings by reason; --verbose expands the details on stderr. Counts refer to top-level settings, including when a nested value is unsupported.

Paths supplied with -o are written atomically. Existing output is refused unless --force is supplied. KDL files are private; generated scripts are private and executable. Generation refuses to overwrite its KDL input.

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

Capture reads explicit persisted preferences, including user/global domains, current-host files, system files, app containers, and separate power profiles. It does not flush application caches or discover every effective default. Broad capture can contain private application state and machine-specific values.

[Coverage](docs/coverage.md) lists supported and unsupported mechanisms. UIDs, fractional dates, exceptional reals, unsafe identities, and some scope combinations remain exact in KDL but cannot currently generate commands. Absolute paths inside values stay literal.

See the [user guide](docs/user-guide.md), [schema](docs/schema.md), and generated [command reference](docs/command-reference.md). The [usage specification](raskin.usage.kdl) is separate from preference KDL.

## Development

See [CONTRIBUTING.md](CONTRIBUTING.md). Tests use synthetic data, generate files, and inspect structure or text. They never run emitted commands or modify actual settings, including in CI or disposable environments.

Fixture verification establishes properties of the files, not that a live Mac adopted the settings.

## License

[MIT](LICENSE).
