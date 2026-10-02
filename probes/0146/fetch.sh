#!/bin/sh
# Record 0146: fetch the pinned cross-encoder/ms-marco-MiniLM-L6-v2 files
# (Apache-2.0) into DIR and check each one's SHA-256. rnx itself makes no
# network call; this is the user's acquisition step (0131's precedent).
set -eu
dir=${1:?usage: fetch.sh DIR}
rev=233902d25c440f23af6f7d6e94d2946bac0bee0a
base=https://huggingface.co/cross-encoder/ms-marco-MiniLM-L6-v2/resolve/$rev
mkdir -p "$dir"
for f in config.json tokenizer.json model.safetensors; do
	curl -sSfL -o "$dir/$f" "$base/$f"
done
cd "$dir"
sha256sum -c <<'SUMS'
380e02c93f431831be65d99a4e7e5f67c133985bf2e77d9d4eba46847190bacc  config.json
d241a60d5e8f04cc1b2b3e9ef7a4921b27bf526d9f6050ab90f9267a1f9e5c66  tokenizer.json
821d1aa69520101d6e0737f78a042ae25b19e5cb9160701909d10434f4aeb0ae  model.safetensors
SUMS
