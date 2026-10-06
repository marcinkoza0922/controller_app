#!/bin/sh
# Lists Rust source files over the line limit (default 400), longest first. Report only.
limit=${1:-400}
cd "$(dirname "$0")/.." || exit 1
find src -name '*.rs' -exec wc -l {} + | awk -v n="$limit" '$2 != "total" && $1 > n' | sort -rn
