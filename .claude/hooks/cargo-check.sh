#!/bin/sh
# After an edit to a Rust file, runs `cargo check` and reports only failures.
file=$(jq -r '.tool_input.file_path // empty')
case "$file" in *.rs) ;; *) exit 0 ;; esac
cd "$CLAUDE_PROJECT_DIR" || exit 0
out=$(cargo check --all-targets --message-format=short 2>&1) && exit 0
echo "$out" | grep -E '^[^ ]+: (error|warning)' | head -20 >&2
exit 2
