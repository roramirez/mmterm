#!/usr/bin/env bash
# Run this INSIDE a running mmterm session to collect baseline numbers.
#
# Usage (inside mmterm):
#   bash bench/run_inside_mmterm.sh
#
# Results are written to /tmp/mmterm_bench_results.txt
# and printed to the terminal as they run.

set -euo pipefail

VTEBENCH=/tmp/vtebench/target/release/vtebench
VTEBENCH_DIR=/tmp/vtebench/benchmarks
TERMBENCH=/tmp/termbench/termbench_release
OUT=/tmp/mmterm_bench_results.txt

echo "=== mmterm benchmark — $(date) ===" | tee "$OUT"
echo "Terminal: ${TERM:-unknown}  Size: $(stty size 2>/dev/null || echo 'unknown')" | tee -a "$OUT"
echo "" | tee -a "$OUT"

# ── vtebench — render throughput ─────────────────────────────────────────────
if [[ -x "$VTEBENCH" ]]; then
    echo "--- vtebench (render throughput) ---" | tee -a "$OUT"
    "$VTEBENCH" -b "$VTEBENCH_DIR" --max-secs 5 2>&1 | tee -a "$OUT"
else
    echo "vtebench not found at $VTEBENCH — skipping" | tee -a "$OUT"
    echo "Build it: cd /tmp/vtebench && cargo build --release" | tee -a "$OUT"
fi
echo "" | tee -a "$OUT"

# ── termbench — data ingestion throughput ────────────────────────────────────
if [[ -x "$TERMBENCH" ]]; then
    echo "--- termbench (data ingestion, regular size) ---" | tee -a "$OUT"
    "$TERMBENCH" regular 2>&1 | tee -a "$OUT"
else
    echo "termbench not found at $TERMBENCH — skipping" | tee -a "$OUT"
    echo "Build it: cd /tmp/termbench && g++ -O3 -Ofast termbench.cpp -o termbench_release" | tee -a "$OUT"
fi
echo "" | tee -a "$OUT"

# ── Plain I/O — 11 MB of sequential output ───────────────────────────────────
echo "--- plain I/O: seq 1 400000 (~11 MB) ---" | tee -a "$OUT"
START=$(date +%s%N)
seq 1 400000 > /dev/null
END=$(date +%s%N)
MS=$(( (END - START) / 1000000 ))
echo "time: ${MS} ms" | tee -a "$OUT"
echo "" | tee -a "$OUT"

# ── Memory ───────────────────────────────────────────────────────────────────
echo "--- memory (RSS of parent mmterm process) ---" | tee -a "$OUT"
# $PPID is the shell's parent — the mmterm process itself
ps -o pid,rss,vsz,comm -p "$PPID" 2>/dev/null | tee -a "$OUT" || \
    echo "(ps lookup failed — check \$PPID manually)" | tee -a "$OUT"
echo "" | tee -a "$OUT"

echo "=== done — results saved to $OUT ===" | tee -a "$OUT"
