# File-defined presets

A preset file lists exactly which stored values to capture. It contains no preference values.
Use it with either `-p` or `--preset`:

~~~sh
raskin capture -p my-preset.kdl -o myprefs.kdl
raskin generate myprefs.kdl -o myprefs.macos
~~~

The file defines the complete selection. Apple is not added implicitly.
Built-in names still work, and Apple remains the default when you omit the option.
Use `./desktop` to select a file named `desktop`, because the bare name selects the built-in.

## Example

~~~kdl
raskin-preset 1

defaults {
    com {
        apple {
            dock {
                autohide
                orientation
                persistent-apps
            }
            finder {
                AppleShowAllFiles
            }
            controlcenter host="current" {
                Battery
            }
        }
    }
}

pmset {
    battery {
        displaysleep
    }
    charger {
        displaysleep
    }
}
~~~

Copy and edit [the starter example](../examples/presets/selected.kdl). Its keys are examples, not recommended values.
The `raskin-preset 1` header distinguishes selection files from captured `raskin 1` value documents.

## Meaning

A bare leaf selects one exact, case-sensitive top-level preference key in its parent domain.
The namespace above it forms the domain name, such as `com.apple.dock`.
Dots in a leaf name remain literal key characters.
Selecting `persistent-apps` captures its whole stored array, including nested values and their types.
A dictionary follows the same rule. Do not add array or dictionary annotations to a selector.

Domain properties match the captured-value schema:

| Property | Default | Meaning |
| --- | --- | --- |
| `scope` | `"user"` | User or system store. The other value is `"system"`. |
| `host` | `"any"` | Unscoped or `"current"` host preferences. |
| `container` | Absent | Exact application container identifier. |

Properties belong on the node directly owning the selected keys.
They do not inherit through namespace branches.
Different scopes need separate domain declarations.
Repeated unscoped namespace branches can contain distinct scoped domains.

~~~kdl
raskin-preset 1
defaults {
    NSGlobalDomain {
        AppleInterfaceStyle
    }
    com {
        example {
            app scope="system" {
                ExampleKey
            }
            app container="com.example.app" {
                ExampleKey
            }
        }
    }
}
~~~

Battery, charger, and UPS keys stay separate under `pmset`.
Only explicit top-level keys are supported. There are no wildcards, includes, category expansion, or nested collection selectors.
An empty child block selects nothing. It never means every key in a domain.
A file with no selected keys captures an empty document and performs no preference-provider reads.

Duplicate scoped domains and duplicate keys are errors.
Values, type annotations, and properties on key leaves are errors.
Invalid files and missing files fail before capture or output. They never fall back to a built-in preset.
The parser uses the same byte and nesting limits as captured KDL.

## Capture behavior

Capture opens only selected stores and domains, then retains selected keys before generation-support checks.
An excluded sibling value cannot force the selected keys into a different native encoding.
Selected arrays and dictionaries remain complete. Unsupported nested values exclude the whole selected top-level setting.

Missing, unreadable, and unsupported fields are omitted from the captured KDL.
Normal stderr summarizes them. `--verbose` lists details.
An unreadable source is reported as unreadable, rather than labeling all its keys missing.
Supported scope and power mappings remain those documented in [coverage](coverage.md).
Raskin does not invent values from a VM, registered defaults, or another capture.

Stored presence or a difference from another machine does not establish that a setting was deliberately changed.
The selection file records which keys you care about.
Use the same selection file for later captures and compare the resulting files with ordinary diff tools.
Generation needs only captured KDL. It does not need the selection file.
