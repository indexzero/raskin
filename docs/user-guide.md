# User guide

Use capture to read source settings and generate to compile saved KDL. The [command reference](command-reference.md) is generated from the CLI usage specification.

## Saved input trees

Offline capture uses an isolated directory:

~~~text
saved-inputs/
  user/
    .GlobalPreferences.plist
    com.example.app.plist
    ByHost/com.example.app.SOURCE-UUID.plist
  system/
    com.example.service.plist
  containers/
    com.example.container/
      com.example.app.plist
  group-containers/
    group.example.app/
  host-id.txt
  pmset.txt
~~~

The user and system directories stand for Library/Preferences. Each containers child stands for that app's Data/Library/Preferences directory. Optional group-containers children account for unsupported stores; their contents are not read.

host-id.txt contains one source UUID. pmset.txt contains native pmset -g custom output with explicit profile headings. Missing/malformed inputs are recorded. There is no fallback to the current machine.

~~~sh
raskin capture --from saved-inputs -o saved.kdl
raskin generate saved.kdl -o saved.macos
~~~

Observed symlinks are refused, including ancestor paths. Use the real path when a temporary-directory alias is a symlink. Metadata checks do not provide an adversarial sandbox against concurrent symlink replacement.

## Failures and partial output

Malformed KDL reports its source, line and column. Unsupported versions fail. Fix input before generating again.

Unavailable capture sources remain in KDL. They do not block generating captured settings. If a retained value has no faithful mapping, generation fails before writing. --skip-unsupported requests partial output with comments and diagnostics naming every skipped setting.

--force replaces output atomically. It does not permit overwriting input, including via hard links or symlinks. A missing parent directory is an error.

Stdout contains only the artifact or usage specification. Diagnostics use stderr. Success returns zero; invalid input/output failures return nonzero. A successful capture can still have unavailable records. Inspect these and the coverage notes.

## Comparison and handoff

Use the same preset on both machines. Compare matching KDL or script files. Existing tools handle comparison, backup, and transfer.

Inspect the generated script before running it yourself. Read [native mappings](native-commands.md) for compound values and scope behavior. No Raskin runtime is needed on the destination.
