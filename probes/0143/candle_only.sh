#!/bin/sh
# Record 0143: E6's synthetic mode for a Candle-only session (`:dep candle`).
# E6's run() builds its table with Polars, so it cannot compile without it;
# this keeps every line before `pub fn run` byte for byte (the k-means, the
# trace, the synthetic entry) and adds a run() that only dispatches to it.
#   candle_only.sh OUT
sed '/^pub fn run(args) {$/,$d' "$(dirname "$0")/e6_topics.rn" > "$1"
printf 'pub fn run(args) { synthetic(args) }\n' >> "$1"
