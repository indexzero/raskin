# Agent instructions

Read [INTENT.md](INTENT.md) before changing project behavior. [RESEARCH.md](RESEARCH.md) records source evidence and the user's Rust CLI preferences.

## Hard boundary: generate and inspect files only

The user explicitly requires:

> WE NEVER RUN THE *.macos shell files ANYWHERE!!!
> We just generate files on disk, that's it

- Never execute a generated `.macos` file, directly or through a shell, sourcing, piping, evaluation, or another interpreter.
- Never run its commands individually or reproduce their settings changes through another API as a testing workaround.
- Never modify actual macOS preferences, power settings, or other system settings during agent testing. Do not restart apps or preference services to validate generated output.
- This rule applies everywhere, including the user's machine, other machines, CI, VMs, disposable accounts, and disposable preference domains. There is no disposable-environment exception.
- Tests use fixtures or mocks, generate files, and inspect their data or text. Parsing, serialization, semantic round trips, snapshots, and static checks must stay within this boundary.
- Do not run upstream installers, restore scripts, or settings mutators from `research/`.

Execution is a human end-user activity outside agent testing. Do not claim that generation-only tests prove live settings were applied.

## Product boundaries

- Raskin reads macOS preferences into KDL and generates `.macos` files from saved KDL. It does not apply KDL or execute the generated commands.
- Native commands define their own behavior. Do not introduce separate application, merge, conflict-resolution, or replay policies.
- Accept file-defined presets as complete exact-key selections, without implicit built-in additions. Keep all named presets as built-ins.
- Default to apple: the union of macOS system categories plus Terminal. Keep broad available with its wider scope. Retain supported unknown fields within selected domains.
- The generated script stands alone; the destination does not need Raskin. Produce plist output only where macOS itself requires it.
- Do not add a diff facility. Document fresh capture/generation followed by ordinary file comparison in the README.
- Backup and transfer belong to the user's existing tools.

## KDL schema approval

Present a concrete KDL schema proposal with representative examples and obtain the owner's approval before schema-dependent implementation. Agent review does not substitute for that approval. Obtain renewed approval before changing the approved schema's syntax or meaning.
