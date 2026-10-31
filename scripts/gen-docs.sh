#!/usr/bin/env bash
# Generate interface artifacts only. Never source completions or execute .macos files.
set -euo pipefail
cd "$(dirname "$0")/.."
mode=write
if [ "$#" -gt 0 ]; then mode="$1"; fi
case "$mode" in write|--check) ;; *) printf 'usage: %s [--check]\n' "$0" >&2; exit 2 ;; esac
raskin_docs_tmp="$(mktemp -d)"
trap 'rm -rf "$raskin_docs_tmp"' EXIT
mkdir -p "$raskin_docs_tmp/docs/man" "$raskin_docs_tmp/completions"
cargo run --quiet --locked -- --usage-spec > "$raskin_docs_tmp/raskin.usage.kdl"
(
    cd "$raskin_docs_tmp"
    usage generate markdown --file raskin.usage.kdl > docs/command-reference.md
    usage generate manpage --file raskin.usage.kdl > docs/man/raskin.1
    for shell in bash zsh fish; do
        usage generate completion "$shell" raskin --file raskin.usage.kdl --usage-cmd 'raskin --usage-spec' > "completions/raskin.$shell"
    done
)
for file in raskin.usage.kdl docs/command-reference.md docs/man/raskin.1 completions/raskin.bash completions/raskin.zsh completions/raskin.fish; do
    if [ "$mode" = --check ]; then
        diff -u "$file" "$raskin_docs_tmp/$file"
    else
        mkdir -p "$(dirname "$file")"
        cp "$raskin_docs_tmp/$file" "$file"
    fi
done
