#!/usr/bin/env bash
# Record 0148: fetch the pinned NLI cross-encoder and check each file's
# SHA-256. rnx itself makes no network call; this is the user's step.
set -euo pipefail
dir=${1:?usage: fetch.sh DIR}
rev=a150876415327c80daeff35ca6f68f5ed8cf5c24
base=https://huggingface.co/cross-encoder/nli-deberta-v3-xsmall/resolve/$rev
mkdir -p "$dir"
for f in config.json tokenizer.json model.safetensors; do
	curl -sSfL -o "$dir/$f" "$base/$f"
done
cd "$dir"
sha256sum -c <<'SUMS'
8d9f07bf7ba54a6fc3b1962483056f94c39dcf188db4cf61843e1c88f94b2342  config.json
5124ef2ead1a10a717703bc436de7f353da76d6340e4587719b42b1693707964  tokenizer.json
4e4fc4977f8d29d2a164255c8f69b9d6c158deeb309bb5e70445b94666ccd9e9  model.safetensors
SUMS
