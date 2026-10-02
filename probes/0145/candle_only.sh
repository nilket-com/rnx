#!/bin/sh
# Record 0145: E7's synthetic mode for a Candle-only session (`:dep candle`).
# E7's run() builds its tables with Polars, so it cannot compile without it;
# this keeps every line before `pub fn run` byte for byte (the method, the
# events, the trace, the synthetic entry) and adds a run() that only
# dispatches to it.
#   candle_only.sh OUT
sed '/^pub fn run(args) {$/,$d' "$(dirname "$0")/e7_traffic.rn" > "$1"
printf 'pub fn run(args) { synthetic(args) }\n' >> "$1"
