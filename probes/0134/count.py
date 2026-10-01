"""Record 0134: the canonical identity list, counted mechanically.

The denominator is every public method of candle-core 0.11.0's `Tensor`:
the literal `pub fn`s inside `impl Tensor` blocks plus the macro-defined
element-wise and broadcast operations (unary_op!, binary_op!, binary_op_scalar!,
broadcast_binary_op!), deduplicated. Each manifest identity must exist in it;
the nn and derived rows are counted apart."""
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
rows = [l.split("\t") for l in pathlib.Path(__file__).with_name("manifest.tsv").read_text().splitlines()[1:]]
core_rows = [r for r in rows if r[0] in "ABCDEF"]
names = [r[1].split("::")[1] for r in core_rows]
missing = [n for n in names if n not in methods]
dupes = [n for n, c in collections.Counter(names).items() if c > 1]
fam = collections.Counter(r[0] for r in core_rows)
print(f"denominator: {len(methods)} public Tensor methods")
print("per family:", dict(sorted(fam.items())))
print(f"core identities: {len(set(names))} ({100 * len(set(names)) / len(methods):.1f}%), missing: {missing}, duplicates: {dupes}")
print("counted apart:", dict(collections.Counter(r[0] for r in rows if r[0] not in "ABCDEF")))
sys.exit(1 if missing or dupes else 0)
