# Raskin intent

Confirmed by the user on 2026-10-30, including the subsequent file-diffing and generation-only testing requirements. This document records intent; it does not prescribe CLI syntax or a KDL schema.

> a locus of attention centric way to read & write macos preferences

Raskin is named in honor of Jef Raskin. Its purpose is to memoize macOS preferences in a compact, editable representation and produce the explicit commands a person would run themselves. The tool should demand very little attention beyond that work.

## Confirmed intent

- **Outcome:** Capture macOS preferences as KDL and generate understandable `.macos` shell files from that saved data.
- **User:** People capturing their Mac's preferences and reproducing them on another machine.
- **Why now:** Existing migration approaches miss settings or make the migration tool itself demand too much attention. The missing battery display timeout is a concrete example.
- **Success:** Apple system categories plus Terminal by default, individually selectable categories, an explicit broad preset, and a standalone generated script that needs no Raskin installation on the destination.
- **Constraints:** Rust implementation; KDL as the durable source; plist output only where macOS requires it; agent testing limited to generating and inspecting files.
- **Out of scope:** Raskin executing settings changes, introducing separate application policies, providing a diff facility, or managing backup and transfer.

## The workflow

```text
macOS settings -- read     --> myprefs.kdl
myprefs.kdl    -- generate --> myprefs.macos
myprefs.*      -- backup   --> Git, a bucket, or other user-chosen storage
myprefs.macos  -- transfer --> new machine
myprefs.macos  -- execute  --> human end user runs the native commands
```

Capture and generation are separate. Saved KDL can be edited and used to regenerate a script without access to the original Mac. Backup and transfer use existing tools chosen by the user.

Apple is the default: the union of system category presets plus Terminal. Other standalone Apple apps and third-party apps remain outside this selection. Broad remains available explicitly with its wider scope. Each selected domain retains supported unknown fields, including counters and learned state. Capture reports omissions and unsupported settings without implying complete coverage.

KDL must retain the meaning of the captured data, including nested structure, value types, and relevant scope. Battery and charger values, for example, must remain distinguishable. A smaller representation is useful only if it preserves that meaning.

## The generated file is the operation

Raskin does not apply KDL. Its job ends at generating files that people inspect and execute themselves.

The user's governing principle is:

> what would running that command do? it does that because it is that

The native commands in the generated file determine what happens, including their effects on existing preferences. Raskin does not add a separate merge, conflict-resolution, or replay policy. Correct generation still matters: commands must faithfully express the captured settings and remain understandable to the person reading them.

The intended handoff is one standalone `.macos` file. No Raskin runtime is required on the destination. Native plist output belongs in scope only where macOS itself requires it, not as a default companion format or an alternative source of truth. Any necessary native plist handling belongs in the generated workflow; the KDL remains authoritative.

## Diffing is ordinary file comparison

The README must explain diffing as generating a new KDL or `.macos` file on the new machine and comparing it with the corresponding existing file using ordinary diff tools.

Compare KDL with KDL to examine recorded preferences, or `.macos` with `.macos` to examine generated commands. Use matching capture scopes or presets. Stable, readable structure makes these comparisons useful in `diff`, Git, or the user's preferred comparison tool.

There is no separate Raskin diff facility. Comparison follows directly from the files the tool produces.

## Testing stops at generated files

**Agents and tests must NEVER run generated `.macos` shell files ANYWHERE.** This includes the developer's Mac, other Macs, CI machines, virtual machines, disposable accounts, and disposable preference domains.

Testing generates and inspects files using fixtures. It may check parsing, type preservation, scope, rendering, quoting, stable ordering, and expected file contents. It must not execute generated output, invoke its commands one by one, or reproduce their settings changes through another API. No agent test may modify actual macOS settings or restart applications or preference services to validate an effect.

Human end users decide when to execute the artifacts themselves. Generation tests establish properties of the files; they must not be described as proof that settings were applied to a live Mac.

## Research and implementation direction

Use the “BE THE BORG” method demonstrated in `remarkable-mcp.rs`: inspect source implementations, absorb useful behavior by capability, preserve provenance, and build a coherent Rust implementation. The [research](RESEARCH.md) records what to take and leave from each project.

Follow the user's Rust CLI preferences recorded there: a reusable library and thin CLI, jdx/usage for the interface specification and generated command documentation, mise, hk, and dist/cargo-binstall release conventions. The preferences document is KDL; the separate usage specification also uses KDL but has a different purpose and schema.

Command names, the detailed KDL schema, exact preset contents, and native command mappings are subsequent design work. They must preserve the boundaries above.

## Owner approval of the KDL schema

The owner has opinions about the KDL schema and wants to review a concrete proposal first. Present the proposed structure, type representation, scope, and representative examples before implementation. Obtain the owner's explicit approval before schema-dependent work. Later changes to the approved syntax or meaning require renewed approval.

## Capture output clarification: 2026-11-12

The owner specified JSON-style omission: unavailable values are absent from saved KDL, as undefined object fields are absent from JSON output. Capture diagnostics belong on stderr. Normal output reports what was saved and summarizes skipped sources; detailed diagnostics are available explicitly. This supersedes the original proposal's inclusion of capture omission records in new capture documents.

## Generation default clarification: 2026-11-12

Generate supported settings by default, omitting complete unsupported top-level settings with a brief summary. The ordinary command must produce the useful artifact without requiring --skip-unsupported. Detailed diagnostics remain opt-in. --strict explicitly requests failure before output when unsupported settings exist. Invalid KDL and output errors still fail.

## Capture support clarification: 2026-11-12

If generation cannot support a setting, capture must omit it. Capture and generation use the same support checks, including domain-wide encoding constraints. Unsupported nested data excludes its whole top-level setting; retained values are never altered to fit. This applies to every preset. New KDL contains supported settings only, and omissions remain capture diagnostics. Existing or edited KDL remains readable.

## Apple preset clarification: 2026-11-16

The owner confirmed Apple as the default, system categories plus Terminal, and retention of unknown supported fields. Apple composes all individual system categories and Terminal. Broad retains its previous wider scope. Domain selection replaces short key lists in desktop and input. Exact category selections and overlaps are documented in docs/presets.md. This supersedes the original broad default and two-or-three-preset limit. It changes no KDL syntax, native mappings, or execution boundary.
