# Preference schema 1

Raskin uses KDL 2 with a separate schema header:

~~~kdl
raskin 1
defaults {
    com {
        apple {
            dock {
                autohide #true
                tilesize 48
                (array)recent-apps {}
            }
            controlcenter host="current" { Battery 2; }
        }
    }
}
pmset {
    battery { displaysleep 5; }
    charger { displaysleep 30; }
}
~~~

## Namespace and data boundaries

Untyped groups below defaults accumulate domain segments. Join them with dots. The example identifies com.apple.dock and com.apple.controlcenter. NSGlobalDomain is one domain node directly below defaults.

A scalar node has one positional value. An explicitly typed (dict) or (array) node begins a collection value. Everything beneath a collection describes data, not another domain segment. Dotted preference keys remain literal keys.

A domain can have both direct preferences and a child namespace branch. Duplicate reconstructed domain identities are rejected, even if declarations have different keys. Repeated prefix groups are permitted when their contained scoped domains are distinct.

## Scope

Scope properties belong on the node directly owning preference values:

| Property | Default | Meaning |
| --- | --- | --- |
| scope | "user" | "user" or "system" preference store |
| host | "any" | "any" or "current" host |
| container | absent | Explicit app container identity, independent of domain |

Scope applies to direct preferences and their nested data. It does not propagate into child namespace branches. A prefix group without direct preferences cannot carry scope. Different scopes can use sibling nodes with the same name.

The source UUID is not the destination selector. Current-host generation uses the native currentHost operation. Identifiers and paths inside values are never rewritten by guessing. Unsupported native scope combinations produce generation diagnostics.

## Values

~~~kdl
raskin 1
defaults {
    org {
        example {
            preferences {
                enabled #true
                count 1
                scale 1.0
                title "Work"
                updated (date-time)"2026-10-30T15:04:05Z"
                bytes (base64)"AAEP"
                reference (plist-uid)42
                (dict)window {
                    width 1280.0
                    (array)tabs "Work" "Notes" "Work"
                    (array)singleTab "Work"
                    (array)emptyTabs {}
                    (dict)emptyOptions {}
                }
                (array)workspaces {
                    (dict)- { name "Writing"; }
                    - "A scalar beside a dictionary"
                }
            }
        }
    }
}
~~~

Boolean, integer, real, string, date, data, and archived UID stay distinct. Integers range from -2^63 through 2^64-1. Reals preserve signed zero and explicit #nan, #inf, and #-inf values. NaN payload bits have no KDL representation.

Dates represent RFC 3339 instants and normalize offsets to UTC. Fractional seconds survive KDL round trips in existing or edited documents; new captures omit fractional dates because generation has no verified faithful mapping. Data is base64 with leading zero bytes retained. Existing or edited UIDs stay explicit in KDL; new captures omit settings containing them. The same capture filter excludes other values or scopes unsupported by generation without changing the codec's types.

Null is not an ordinary plist value. Unknown annotations, out-of-range values, malformed dates/base64, and ambiguous value shapes are errors.

## Collections and metadata

Empty arrays and dictionaries require explicit {} blocks. Scalar arrays use positional values. Compound or mixed arrays use child nodes named - for every element. Do not mix positional and child elements. Array order and duplicates are data. Dictionary keys are unique and retain their exact names.

A setting can carry descriptive named properties:

~~~kdl
autohide #true inline="explanatory metadata"
~~~

Metadata never becomes a preference or changes execution behavior. There is no mandatory vocabulary. The illustrative inline=meta in design examples is a placeholder, not capture data. Capture omits absent metadata. Generation does not modify its input.

## Power and capture diagnostics

Power profiles are separate battery, charger, and ups groups. Unknown profiles or keys never become guessed commands or an all-profiles operation.

Capture writes only settings it obtained. Unavailable values and capture bookkeeping are omitted from KDL. Diagnostics stay on stderr: a short summary by default, per-source detail with --verbose. Preset filtering remains intentional selection.

For compatibility, earlier saved files may still contain unavailable nodes with one source string and a reason of permission-denied, unsupported, missing, or read-failed. The decoder accepts these records and generation treats them as inert comments. New captures never emit them.

## Validation and comparison

The file needs exactly one raskin 1 header and at most one defaults and pmset group. Unknown root groups, duplicate metadata/scope properties, duplicate dictionary keys, and duplicate scoped domains are errors.

Canonical capture groups shared prefixes and sorts scopes/dictionary keys. Arrays are never sorted or deduplicated. Normal output adds no timestamps or source usernames. Generation does not save changes to its source text.

Inputs are limited to 128 MiB and 128 levels of nesting. Empty preference dictionaries have no stored keys and are omitted with a diagnostic.

See the synthetic [type fixture](../tests/fixtures/schema/nested-values.kdl) and [scope fixture](../tests/fixtures/schema/scopes-and-omissions.kdl).
