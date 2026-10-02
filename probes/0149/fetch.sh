#!/usr/bin/env bash
# Record 0149: fetch the pinned instruct model and check each file's SHA-256.
# rnx itself makes no network call; this is the user's step.
set -euo pipefail
dir=${1:?usage: fetch.sh DIR}
rev=7ae557604adf67be50417f59c2c2f167def9a775
base=https://huggingface.co/Qwen/Qwen2.5-0.5B-Instruct/resolve/$rev
mkdir -p "$dir"
for f in config.json generation_config.json tokenizer.json tokenizer_config.json model.safetensors; do
	curl -sSfL -o "$dir/$f" "$base/$f"
done
cd "$dir"
sha256sum -c <<'SUMS'
18e18afcaccafade98daf13a54092927904649e1dd4eba8299ab717d5d94ff45  config.json
e558847a8b4402616f1273797b015104dc266fe4b520056fca88823ba8f8ebe6  generation_config.json
c0382117ea329cdf097041132f6d735924b697924d6f6fc3945713e96ce87539  tokenizer.json
5b5d4f65d0acd3b2d56a35b56d374a36cbc1c8fa5cf3b3febbbfabf22f359583  tokenizer_config.json
fdf756fa7fcbe7404d5c60e26bff1a0c8b8aa1f72ced49e7dd0210fe288fb7fe  model.safetensors
SUMS
