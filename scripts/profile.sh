#!/bin/sh
# Where the CPU time goes, and how often the process wakes up. Output goes to
# target/profile/ (already git-ignored); the report is printed as well.
#
#   scripts/profile.sh bench [filter]        perf over the benchmarks (see scripts/bench.sh)
#   scripts/profile.sh pid <pid> [seconds]   perf sample of a running process (default 10 s)
#   scripts/profile.sh stats <pid> [seconds] CPU %, wakeups/s, memory and threads (default 10 s)
#
# Find a pid with `pgrep -a controller_app`. Sampling a process needs perf_event access;
# for the desktop's own processes that is usually allowed.
cd "$(dirname "$0")/.." || exit 1
out=target/profile
mkdir -p "$out"

report() {
    perf report -i "$1" --stdio --no-children -g none --percent-limit 0.5 2>/dev/null | grep -v '^#' | grep . | head -"${2:-40}"
}

case "$1" in
bench)
    # Profile the test binary itself, so cargo and nextest stay out of the report. Find it by
    # listing each test binary's tests; scripts/bench.sh builds it first.
    bin=$(for f in target/release/deps/controller_app-*; do
        [ -x "$f" ] && [ ! -d "$f" ] && "$f" --list 2>/dev/null | grep -q 'bench_' && echo "$f" && break
    done)
    [ -n "$bin" ] || { echo "no test binary; run scripts/bench.sh first" >&2; exit 1; }
    perf record -F 999 -o "$out/bench.data" -- "$bin" "bench_$2" --ignored --nocapture --test-threads 1 >"$out/bench.log" 2>&1
    grep '^bench ' "$out/bench.log"
    echo "--- hottest symbols (all cases together) ---"
    report "$out/bench.data"
    ;;
pid)
    seconds=${3:-10}
    perf record -F 499 -g -p "$2" -o "$out/pid-$2.data" -- sleep "$seconds" >/dev/null 2>&1
    report "$out/pid-$2.data"
    ;;
stats)
    seconds=${3:-10}
    stat() { awk '{print $14+$15}' "/proc/$2/stat"; }
    ctx() { awk '/voluntary_ctxt_switches/ {print $2}' "/proc/$2/status" | paste -sd' ' -; }
    t0=$(stat x "$2"); c0=$(ctx x "$2")
    sleep "$seconds"
    t1=$(stat x "$2"); c1=$(ctx x "$2")
    set -- "$@"
    awk -v t0="$t0" -v t1="$t1" -v c0="$c0" -v c1="$c1" -v s="$seconds" -v tick="$(getconf CLK_TCK)" -v pid="$2" '
        BEGIN {
            split(c0, a, " "); split(c1, b, " ");
            printf "pid %s over %d s\n", pid, s;
            printf "  cpu            %.2f %%\n", (t1 - t0) / tick / s * 100;
            printf "  wakeups        %.1f /s (voluntary context switches)\n", (b[1] - a[1]) / s;
        }'
    grep -E '^(VmRSS|Threads)' "/proc/$2/status" | sed 's/^/  /'
    ;;
*)
    sed -n '2,9p' "$0" | sed 's/^# \{0,1\}//'
    exit 2
    ;;
esac
