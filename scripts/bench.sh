#!/bin/sh
# Runs the timing benchmarks in release mode and prints one "bench <case> <time>/iter" line
# per case. Benchmarks live in src/bench.rs and src/gui/bench.rs and are #[ignore]d.
#   scripts/bench.sh          all cases
#   scripts/bench.sh gui      only the settings-window cases
cd "$(dirname "$0")/.." || exit 1
filter=${1:-}
RUST_BACKTRACE=0 cargo nextest run --release --run-ignored only --no-capture --test-threads 1 --hide-progress-bar --cargo-quiet "bench_$filter" 2>&1 | grep -E '^bench |panicked|FAIL|error'
