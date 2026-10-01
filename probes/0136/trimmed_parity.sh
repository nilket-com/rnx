#!/bin/sh
# Record 0136, review round 2: passage parity with the twin under a
# tokenizer whose spaces are empty-span tokens (byte-level, trimmed offsets).
#
#   trimmed_parity.sh PROBE TWIN MODEL WORK
#
# MODEL is the pinned model; WORK gets a copy of it with review round 1's
# tokenizer, a small D1 (documents) and D2 (tickets) of the edge cases, and
# both implementations' passage ranges, which must be identical at overlap 0
# and 32, with every rnx plan's checks passing (probe0136 exits 1 otherwise).
set -eu
probe=$1; twin=$2; model=$3; work=$4
rm -rf "$work"; mkdir -p "$work/d1"
cp -r "$model" "$work/model"
python3 - "$work" <<'EOF'
import json, pathlib, sys
w = pathlib.Path(sys.argv[1])
tok = {
    "version": "1.0", "truncation": None, "padding": None, "added_tokens": [],
    "normalizer": None,
    "pre_tokenizer": {"type": "ByteLevel", "add_prefix_space": False, "trim_offsets": True, "use_regex": True},
    "post_processor": {"type": "ByteLevel", "add_prefix_space": False, "trim_offsets": True, "use_regex": True},
    "decoder": None,
    "model": {"type": "BPE", "dropout": None, "unk_token": "[UNK]",
              "continuing_subword_prefix": None, "end_of_word_suffix": None,
              "fuse_unk": False, "byte_fallback": False, "ignore_merges": False,
              "vocab": {"[PAD]": 0, "[UNK]": 1, "Ġ": 2, "a": 3}, "merges": []},
}
(w / "model/tokenizer.json").write_text(json.dumps(tok))
cases = [" a", "a ", "a  ", " a a ", "a   a", " a" * 200, "a " * 300, " a  a " * 90]
for i, c in enumerate(cases):
    (w / "d1" / f"{i:04}_case.md").write_text(f"# case {i}\n{c}")
(w / "d2.json").write_text(json.dumps(
    [{"number": i, "title": c, "body": c} for i, c in enumerate(cases)]))
EOF
for o in 0 32; do
	"$probe" chunks "$work/model" "$work/d1" "$work/d2.json" $o > "$work/rnx-$o.tsv"
	"$twin" chunks "$work/d1" "$work/d2.json" "$work/model" $o > "$work/twin-$o.tsv"
	if grep -q REFUSED "$work/twin-$o.tsv"; then echo "the twin refused a case"; exit 1; fi
	cmp "$work/rnx-$o.tsv" "$work/twin-$o.tsv"
	echo "overlap $o: $(($(wc -l < "$work/rnx-$o.tsv") - 1)) passage ranges identical to the twin"
done
