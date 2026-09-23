#!/usr/bin/env python3
"""Record 0083 gate 1: the function-level generic census, reproducibly.

  probes/0083/census.py [--out probes/0083/census.json]

Joins the pinned adapter-narrow inventory to adapters/polars/surface.json
by inventory key and reports: the generics partition (with generics /
extractor-unsupported / eligible), each eligible callable's surface
disposition, the role of every generic parameter (argument, return-only,
closure, associated), and the separate receiver-pair census of pairs the
applicability engine left unresolved for function-level generics. Every
candidate is listed by key and canonical path. The run fails if the
inventory's provenance or resolved feature set differs from the shipped
release file's.
"""
import json, re, sys, collections, os
root = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
INV = os.path.join(root, "probes/0072/out/0.55.2-adapter-narrow/result/inventory.json")
SURFACE = os.path.join(root, "adapters/polars/surface.json")
RELEASE = os.path.join(root, "tools/polars-gen/releases/0.55.2-joins.toml")
out = sys.argv[sys.argv.index("--out") + 1] if "--out" in sys.argv else os.path.join(root, "probes/0083/census.json")

inv = json.load(open(INV)); surface = json.load(open(SURFACE))
# provenance: the release file's [provenance] block, parsed minimally
rel = open(RELEASE).read()
prov = rel.split("[provenance]", 1)[1].split("\n[", 1)[0]
want_release = re.search(r'^release\s*=\s*"([^"]+)"', prov, re.M).group(1)
want_cfg = re.search(r'^cfg\s*=\s*"([^"]+)"', prov, re.M).group(1)
want_features = sorted(re.findall(r'"([^"]+)"', re.search(r'^features\s*=\s*\[(.*)\]', prov, re.M).group(1)))
have = inv["provenance"]
if (have.get("release"), have.get("cfg"), sorted(have.get("features") or [])) != (want_release, want_cfg, want_features):
    sys.exit(f"FAIL: inventory provenance {have} does not match the release file ({want_release}, {want_cfg}, {want_features})")
if surface["release"]["name"] != want_release:
    sys.exit(f"FAIL: surface.json was generated for release {surface['release']['name']!r}, not {want_release!r}")

entries = {e["key"]: e for e in surface["entries"]}
callables = inv["callables"]
with_generics = [c for c in callables if c.get("generics_canonical")]
extractor_unsupported = [c for c in with_generics if c["bucket"] == "unsupported"]
eligible = [c for c in with_generics if c["bucket"] != "unsupported"]

def mentions(ty, key):
    return re.search(rf"(?<![A-Za-z0-9_:]){re.escape(key)}(?![A-Za-z0-9_])", ty or "") is not None

def roles(c):
    """Role of each function-level generic: where it can be chosen from."""
    out = {}
    params = [p["ty_canonical"] for p in c["params"]]
    ret = c.get("ret_canonical") or ""
    for key, bound in c["generics_canonical"]:
        in_args = any(mentions(t, key) for t in params)
        in_ret = mentions(ret, key)
        closure = bool(re.search(r"Fn(Mut|Once)?\(|core::ops::function::Fn|FnMut<|BinaryFnMut|TernaryFnMut|UnaryFnMut", bound or ""))
        assoc = "::" in key
        # an `impl Trait` argument is an anonymous generic whose key is its own spelling
        anonymous = key.startswith("impl ") and any(key in (t or "") for t in params)
        if closure: role = "closure"
        elif key == "Self": role = "self-bound"          # a trait method on Self (iterator extension traits)
        elif assoc: role = "associated"
        elif in_args or anonymous: role = "argument"
        elif in_ret: role = "return-only"
        else: role = "unbound"
        out[key] = {"role": role, "bound": bound, "in_args": in_args, "in_ret": in_ret}
    return out

SCALAR = {"bool", "i64", "f64", "usize", "i8", "i16", "i32", "u8", "u16", "u32", "u64", "f32", "str", "alloc::string::String", "polars_utils::pl_str::PlSmallStr", "polars_utils::index::IdxSize"}
WRAPPED_HINTS = ("polars_plan::dsl::expr::Expr", "polars_core::series::Series", "polars_core::frame::column::Column", "polars_core::datatypes::field::Field", "polars_core::datatypes::dtype::DataType")
FOREIGN = ("chrono::", "rayon::", "object_store::", "std::io::", "core::io::", "alloc::io::", "serde_core::", "std::path::", "polars_arrow::array::Array", "core::hash::Hasher", "Udf")

def item_of(bound):
    """The `Item = …` of an iterator bound, without the trailing `+ Bound`s."""
    m = re.search(r"Item = (.*)$", bound or "")
    if not m: return None
    item = m.group(1)
    depth = 0
    for i, ch in enumerate(item):
        if ch == "<": depth += 1
        elif ch == ">":
            if depth == 0: return item[:i]
            depth -= 1
    return item

def decide(c, r, dry_reason):
    """Gate 2: one decision per unavailable callable, from its generic
    roles and bounds. Candidates name the concrete type the release policy
    would choose; refusals name the contract still to be settled."""
    gens = r["generics"]; bounds = {k: (v["bound"] or "") for k, v in gens.items()}
    roles = set(v["role"] for v in gens.values())
    other = dry_reason or ""
    if any(other.startswith(x) for x in ("receiver consumes a non-Clone", "name taken", "hand-written", "generic owner", "owner has no public path", "lifetime owner", "no wrapped implementor", "on every implementor: no wrapped", "unreachable polars type", "generic type: f", "mutable reference", "foreign type", "owner not wrapped")):
        if "closure" not in roles or other.startswith("generic type: f"):
            return "blocked elsewhere", other.split(" (")[0][:70], None
    if "closure" in roles:
        return "callback audit", "a closure bound needs a 0079 invocation/mutation/sink audit entry", None
    if "self-bound" in roles and roles <= {"self-bound"}:
        return "refused: Self bound", "a trait method whose only generic is Self (iterator extension or marker trait)", None
    if "return-only" in roles and "argument" not in roles:
        return "refused: return-only", "the generic is chosen only by a return annotation the script cannot express", None
    if any(f in b for b in bounds.values() for f in FOREIGN) or any(f in (item_of(b) or "") for b in bounds.values() for f in FOREIGN) or any(re.search(r"\bArray\b|Array = ", b) for b in bounds.values()):
        return "refused: foreign element", "a chrono, rayon, io, path, serde or Arrow value has no script conversion", None
    # chained: I: IntoIterator<Item = S> with S: AsRef<str> / Into<PlSmallStr>; E: AsRef<[IE]> with IE: Into<Expr>
    concrete = {}
    for k, b in bounds.items():
        it = item_of(b)
        m_slice = re.search(r"AsRef<\[(\w+)\]>", b)
        if "IntoIterator" in b and it and it in bounds:
            inner = bounds[it]
            # exactly a string bound: `Into<(PlSmallStr, Field)>` (Schema::from_iter_check_duplicates) is a tuple with a Field, not a string
            if re.fullmatch(r"core::convert::AsRef<str>|core::convert::Into<polars_utils::pl_str::PlSmallStr>", inner.strip()): concrete[k] = "Vec<String>"; concrete[it] = "String"
            elif "Into<polars_plan::dsl::expr::Expr>" in inner: concrete[k] = "Vec<Expr>"; concrete[it] = "Expr"
        elif m_slice and m_slice.group(1) in bounds and "Into<polars_plan::dsl::expr::Expr>" in bounds[m_slice.group(1)]:
            concrete[k] = "Vec<Expr>"; concrete[m_slice.group(1)] = "Expr"
        elif "IntoIterator" in b and it and re.fullmatch(r"\((\w+), (\w+)\)", it):
            a, b2 = re.fullmatch(r"\((\w+), (\w+)\)", it).groups()
            if all(x in bounds and re.fullmatch(r"core::convert::AsRef<str>|core::convert::Into<polars_utils::pl_str::PlSmallStr>", bounds[x].strip()) for x in (a, b2)):
                concrete[k] = "Vec<(String, String)>"; concrete[a] = "String"; concrete[b2] = "String"
    if concrete and all(k in concrete for k in bounds):
        return "candidate: chained inference", "the item generic is bounded through the container generic; both resolve to owned script values", concrete
    if c["canonical_path"] == "polars_core::chunked_array::ChunkedArray::match_chunks":
        return "refused: unchecked precondition", "Polars enforces the single-chunk receiver and the chunk-length sum with debug_assert! only and slices unchecked (polars-core 0.55.2 chunked_array/mod.rs:851-866); a binding must validate both or stay refused (probe B)", None
    if any("Into<(" in b for b in bounds.values()):
        return "refused: bound", "the item converts into a tuple holding a wrapped value (`Into<(PlSmallStr, Field)>`); needs a proven tuple-and-Field input mapping", None
    if any("impl " in (item_of(b) or "") or "PolarsResult<" in (item_of(b) or "") for b in bounds.values()):
        return "refused: iterator item", "items that are themselves anonymous generics or results have no script conversion", None
    family_trait = any(t in b for b in bounds.values() for t in ("PolarsNumericType", "PolarsDataType", "PolarsIntegerType", "PolarsCategoricalType")) or any(("::Native" in (item_of(b) or "") or "::Physical" in (item_of(b) or "")) for b in bounds.values()) or (c.get("owner_generic") and any(b.startswith("core::convert::AsRef<T>") or re.fullmatch(r"(core::option::Option<)?N>?", item_of(b) or "") for b in bounds.values()))
    if any(re.fullmatch(r"(core::option::Option<)?[A-Z]>?", item_of(b) or "") for b in bounds.values()) and not family_trait:
        return "refused: free element generic", "the item type is a free generic (`T: Ord`) with no Polars family to instantiate from; a policy would have to name it", None
    # iterator inputs: a bare Iterator / ExactSizeIterator / TrustedLen bound, or an
    # IntoIterator whose item is concrete but was not admitted by today's rule
    if any(re.search(r"Iterator<Item = |TrustedLen", b) for b in bounds.values()):
        items = [item_of(b) for b in bounds.values() if item_of(b)]
        if any(i and re.search(r"(?<![A-Za-z0-9_])[A-Z]\b|::Native|::Physical", i) and not any(w in i for w in WRAPPED_HINTS) for i in items):
            return "candidate: family instantiation", "the item is the owner's or a function generic (`T::Native`, `N`); needs the family applicability engine applied to the function generic", None
        if any(i and (i.startswith("&[") or i.startswith("&str") or i.startswith("core::option::Option<&str") or i.startswith("core::option::Option<&[")) for i in items):
            return "candidate: iterator input, borrowed items" + (" (per family)" if c.get("owner_generic") else ""), "a script vector of owned bytes or strings, borrowed for the call (`iter().map(as_slice|as_str)`)", {k: f"Vec<owned {item_of(b)}>::iter()" for k, b in bounds.items() if item_of(b)}
        if any(i and i.startswith("&") for i in items):
            return "refused: iterator of borrowed wrappers", "items borrow wrapped values (`&Series`, `&Row`); needs a borrow-lifetime rule", None
        if c.get("owner_generic"):
            return "candidate: iterator input, owned items (per family)", "`Vec<Item>::into_iter()` on a generic owner: one binding per proven family pair", {k: f"Vec<{item_of(b)}>::into_iter()" for k, b in bounds.items() if item_of(b)}
        if any(i and (i.split("<")[0] in SCALAR or i.replace("core::option::Option<", "").rstrip(">") in SCALAR or any(w in i for w in WRAPPED_HINTS)) for i in items):
            return "candidate: iterator input, owned items", "`Vec<Item>::into_iter()` satisfies Iterator, ExactSizeIterator and TrustedLen", {k: f"Vec<{item_of(b)}>::into_iter()" for k, b in bounds.items() if item_of(b)}
        return "refused: iterator item", f"item {items} has no script conversion", None
    if any("NumCast" in b for b in bounds.values()):
        return "policy: numeric scalar", "`N: Num + NumCast` from a script number needs the release file to choose the concrete type (f64 or per family)", None
    if any(("PolarsNumericType" in b or "PolarsDataType" in b or "PolarsIntegerType" in b or "PolarsCategoricalType" in b or b.startswith("core::convert::AsRef<T>")) for b in bounds.values()):
        return "candidate: family instantiation", "a function generic over a Polars type family (`T: PolarsNumericType`, `&ChunkedArray<T>`) needs the family applicability engine applied to function generics", None
    if any(b.strip() == "core::iter::traits::collect::IntoIterator" for b in bounds.values()):
        return "refused: iterator without item", "an `IntoIterator` bound with no item type cannot be chosen", None
    if roles <= {"argument"} and other.startswith("bucket"):
        return "refused: bound", f"today's inference admits Into/AsRef/IntoIterator only; bounds {list(bounds.values())[:2]}", None
    if "unbound" in roles or any(b == "" for b in bounds.values()):
        return "refused: unbounded generic", "a generic with no bound cannot be chosen", None
    return "refused: other", other.split(" (")[0][:70], None

rows = []
for c in eligible:
    e = entries.get(c["key"])
    r = roles(c)
    e_status = e["status"] if e else "missing"
    decision = decide(c, {"generics": r}, e.get("reason") if e else None) if e_status in ("unsupported",) else (("available", "", None) if e_status in ("generated", "adapted") else ("out of scope", "internal crate", None))
    rows.append({
        "decision": decision[0], "why": decision[1], "concrete": decision[2],
        "key": c["key"], "canonical_path": c["canonical_path"], "krate": c["krate"], "bucket": c["bucket"],
        "receiver": c["receiver"], "owner_generic": c.get("owner_generic", False),
        "generics": r, "role_set": sorted(set(v["role"] for v in r.values())),
        "surface_status": e["status"] if e else "missing", "surface_bucket": e["bucket"] if e else None,
        "surface_reason": (e.get("reason") if e else None), "bindings": (e.get("rune") if e else None),
    })
partition = {"with_generics": len(with_generics), "extractor_unsupported": len(extractor_unsupported), "eligible": len(eligible),
             "eligible_canonical_paths": len(set(c["canonical_path"] for c in eligible))}
by_status = collections.Counter(r["surface_status"] for r in rows)
unsupported_rows = [r for r in rows if r["surface_status"] == "unsupported"]
by_reason = collections.Counter(re.sub(r"`[^`]*`", "`_`", r["surface_reason"] or "").split(":")[0] for r in unsupported_rows)
by_bucket = collections.Counter(r["surface_bucket"] for r in unsupported_rows)
role_census = collections.Counter(tuple(r["role_set"]) for r in rows if r["surface_status"] in ("unsupported", "adapted"))
pairs = surface["instantiation"]["pairs"]
unresolved_fg = [p for p in pairs if p["disposition"].startswith("unresolved") and "function-level generics" in p["disposition"]]
family = {"pairs": len(unresolved_fg), "keys": len(set(p["key"] for p in unresolved_fg)),
          "by_method": collections.Counter(p["method"] for p in unresolved_fg)}
decisions = collections.Counter(r["decision"] for r in rows if r["surface_status"] == "unsupported")
# the unresolved family methods get the same decision, per method
by_key = {c["key"]: c for c in callables}
family_decisions = {}
for p in unresolved_fg:
    c = by_key[p["key"]]
    rr = {"generics": roles(c)}
    family_decisions.setdefault(c["canonical_path"], {"pairs": 0, "decision": decide(c, rr, "bucket: generic")[0]})["pairs"] += 1
family_by_decision = collections.Counter()
for v in family_decisions.values(): family_by_decision[v["decision"]] += v["pairs"]
report = {
    "provenance": have, "partition": partition, "surface_status": dict(by_status),
    "decisions": dict(decisions), "family_decisions": family_decisions, "family_pairs_by_decision": dict(family_by_decision),
    "unsupported_by_bucket": dict(by_bucket), "unsupported_by_reason": dict(by_reason),
    "unavailable_by_generic_roles": {" + ".join(k): v for k, v in sorted(role_census.items())},
    "family_unresolved": {"pairs": family["pairs"], "keys": family["keys"], "by_method": dict(family["by_method"])},
    "scoreboard_0082": {"available": 2074, "value_tested": 1312, "unsupported": 1969, "refused_proven_pairs": 287},
    "rows": sorted(rows, key=lambda r: (r["surface_status"], r["canonical_path"], r["key"])),
}
json.dump(report, open(out, "w"), indent=1)
print(f"partition: {partition}")
print(f"surface status of the eligible: {dict(by_status)}")
print(f"unsupported by bucket: {dict(by_bucket)}")
print(f"unsupported by reason: {dict(by_reason)}")
print(f"unavailable, by generic roles: {report['unavailable_by_generic_roles']}")
print(f"family pairs unresolved for function-level generics: {family['pairs']} across {family['keys']} keys")
print(f"decisions for the unavailable: {dict(decisions)}")
print(f"family pairs by decision: {dict(family_by_decision)}")
print(f"written: {out}")
