"""Record 0148: the `:dep candle` check. Each logit dep_candle.rn wrote must
equal, in f32 bits, the saved early-gate "alone" logit at the same place.

  dep_compare.py LOGITS EARLY_GATE_JSON"""
import json, struct, sys

got = open(sys.argv[1]).read().splitlines()
want = json.load(open(sys.argv[2]))["alone"]
f32 = lambda x: struct.pack("<f", float(x))
if len(got) != len(want) or not want:
    sys.exit(f"FAIL: {len(got)} logits, want {len(want)}")
diff = [i for i, (a, b) in enumerate(zip(got, want)) if f32(a) != f32(b)]
if diff:
    sys.exit(f"FAIL: {len(diff)} logits differ, first at {diff[0]}")
print(f"BIT-EQUAL: {len(want)} NLI logits ({len(want) // 3} pairs, each alone), in f32 bits, against the saved early gate")
