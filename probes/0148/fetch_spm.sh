#!/usr/bin/env bash
# Record 0148: the model's SentencePiece file, for the tokenizer diagnostics
# only (section 6a); rnx does not load it.
set -euo pipefail
dir=${1:?usage: fetch_spm.sh DIR}
curl -sSfL -o "$dir/spm.model" https://huggingface.co/cross-encoder/nli-deberta-v3-xsmall/resolve/a150876415327c80daeff35ca6f68f5ed8cf5c24/spm.model
cd "$dir"
sha256sum -c <<'SUMS'
c679fbf93643d19aab7ee10c0b99e460bdbc02fedf34b92b05af343b4af586fd  spm.model
SUMS
