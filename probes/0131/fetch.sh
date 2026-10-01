#!/bin/sh
# Record 0131: fetch the pinned all-MiniLM-L6-v2 files into DIR and check
# each one's SHA-256. rnx itself makes no network call; this is the user's
# acquisition step. model.safetensors' hash equals Hugging Face's LFS oid.
set -eu
dir=${1:?usage: fetch.sh DIR}
rev=1110a243fdf4706b3f48f1d95db1a4f5529b4d41
base=https://huggingface.co/sentence-transformers/all-MiniLM-L6-v2/resolve/$rev
mkdir -p "$dir/1_Pooling"
for f in config.json tokenizer.json model.safetensors modules.json 1_Pooling/config.json sentence_bert_config.json; do
	curl -sSfL -o "$dir/$f" "$base/$f"
done
cd "$dir"
sha256sum -c <<'SUMS'
953f9c0d463486b10a6871cc2fd59f223b2c70184f49815e7efbcab5d8908b41  config.json
be50c3628f2bf5bb5e3a7f17b1f74611b2561a3a27eeab05e5aa30f411572037  tokenizer.json
53aa51172d142c89d9012cce15ae4d6cc0ca6895895114379cacb4fab128d9db  model.safetensors
84e40c8e006c9b1d6c122e02cba9b02458120b5fb0c87b746c41e0207cf642cf  modules.json
4be450dde3b0273bb9787637cfbd28fe04a7ba6ab9d36ac48e92b11e350ffc23  1_Pooling/config.json
fc1993fde0a95c24ec6c022539d41cf6e2f7c9721e5415d6fb6897472a9cd4b7  sentence_bert_config.json
SUMS
