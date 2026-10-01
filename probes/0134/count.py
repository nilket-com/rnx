"""Record 0134: the canonical identity list, counted mechanically.

The denominator is every public method of candle-core 0.11.0's `Tensor`:
the literal `pub fn`s inside `impl Tensor` blocks plus the macro-defined
element-wise and broadcast operations (unary_op!, binary_op!, binary_op_scalar!,
broadcast_binary_op!), deduplicated. Each manifest identity must exist in it;
the nn and derived rows are counted apart.

  count.py CANDLE_CORE_SRC [MANIFEST...]

With no manifests, 0134's own. With several (record 0137: 0134's then
0137's), they are counted cumulatively, per manifest and in total, and an
identity in two manifests is a duplicate."""
import collections, pathlib, re, sys

core = pathlib.Path(sys.argv[1])
methods = set()
for p in core.rglob("*.rs"):
    s = p.read_text()
    for m in re.finditer(r'impl(?:<[^>]*>)?\s+Tensor\s*\{', s):
        depth, i = 0, m.end() - 1
        while i < len(s):
            depth += {'{': 1, '}': -1}.get(s[i], 0)
            if depth == 0:
                break
            i += 1
        methods |= set(re.findall(r'\n    pub fn ([a-z_0-9]+)', s[m.end():i]))
    methods |= set(re.findall(r'(?:unary_op|binary_op|binary_op_scalar|broadcast_binary_op)!\(\s*([a-z_0-9]+)', s))
CORE = set("ABCDEFGHI")
paths = [pathlib.Path(p) for p in sys.argv[2:]] or [pathlib.Path(__file__).with_name("manifest.tsv")]
rows = []
for p in paths:
    part = [l.split("\t") for l in p.read_text().splitlines()[1:] if l]
    bad = [r for r in part if len(r) != 3 or not (r[0] in CORE or r[0] in ("nn", "derived"))]
    if bad:
        print(f"{p}: malformed rows {bad[:3]}")
        sys.exit(1)
    n = len({r[1] for r in part if r[0] in CORE})
    print(f"{p}: {n} core identities")
    rows += part
core_rows = [r for r in rows if r[0] in CORE]
names = [r[1].split("::")[1] for r in core_rows]
missing = [n for n in names if n not in methods]
dupes = [n for n, c in collections.Counter(names).items() if c > 1]
fam = collections.Counter(r[0] for r in core_rows)
print(f"denominator: {len(methods)} public Tensor methods")
print("per family:", dict(sorted(fam.items())))
print(f"core identities: {len(set(names))} ({100 * len(set(names)) / len(methods):.1f}%), missing: {missing}, duplicates: {dupes}")
print("counted apart:", dict(collections.Counter(r[0] for r in rows if r[0] not in CORE)))
sys.exit(1 if missing or dupes else 0)
