"""Record 0146: the `:dep candle` check. Each score dep_candle.rn wrote must
equal, in f32 bits, the cross-encoder score of the same pair in a validated
U5 trace (in order: query, then retrieval rank).

  dep_compare.py SCORES U5_TRACE"""
import struct, sys

got = open(sys.argv[1]).read().splitlines()
want = [l.split("\t")[6] for l in open(sys.argv[2]).read().splitlines() if l.startswith("cand\t")]
f32 = lambda x: struct.pack("<f", float(x))
if len(got) != len(want) or not want:
    sys.exit(f"FAIL: {len(got)} scores, want {len(want)}")
diff = [i for i, (a, b) in enumerate(zip(got, want)) if f32(a) != f32(b)]
if diff:
    sys.exit(f"FAIL: {len(diff)} scores differ, first at pair {diff[0]}")
print(f"BIT-EQUAL: {len(want)} cross-encoder scores, in f32 bits, against the U5 trace")
