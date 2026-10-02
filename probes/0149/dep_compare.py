"""Record 0149: the `:dep candle` check. Each chat's ids from dep_candle.rn
must equal the saved early gate's ids for the same chat.

  dep_compare.py IDS_TXT GATE_JSON"""
import json, sys

got = [l for l in open(sys.argv[1]).read().splitlines()]
want = [",".join(str(i) for i in c["ids"]) for c in json.load(open(sys.argv[2]))]
if got != want:
    sys.exit(f"FAIL: {sum(a != b for a, b in zip(got, want))} of {len(want)} chats differ (or the counts: {len(got)} against {len(want)})")
print(f"IDENTICAL: {len(want)} chats' greedy ids ({sum(len(w.split(',')) for w in want)} tokens) against the saved early gate")
