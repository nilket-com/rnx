#!/usr/bin/env python3
"""Record 0125: translate rustdoc keys from the adapter-narrow inventory to the
adapter-json one.

A callable's `key` is a rustdoc item id (`crate:item` or `crate:impl:item`).
Rustdoc renumbers items when a crate gains modules, and `json` adds modules to
polars-io, so 1,127 unchanged callables get new keys. The freeze and 40 release
rows name keys, so they are translated here; nothing else changes.

  rekey.py <old inventory> <new inventory> <frozen.json> <release.toml>

Each old key maps to the new key of the callable with the same identity: every
inventory field except the closed NOT_IDENTITY list (numbering, docs,
reachability, the classifier's opinions), so impl_assoc, trait_lifetimes and
any later field count. An identity must be unique in both inventories, or the
run refuses. A keyed release row's `path` must be the mapped callable's
canonical path. Both files are rewritten in place; the counts are printed.
"""
import json
import re
import sys

# Review of 0125: the identity is every field an inventory records, minus
# this closed list, so a field added later (or present on few callables, as
# `trait_lifetimes` is) is part of it by default. Excluded, each for a reason:
NOT_IDENTITY = {
    "key": "rustdoc's numbering, the thing being translated",
    "docs_first": "documentation text",
    "found_paths": "where the item is reachable from: a new feature may add re-exports",
    "crate_paths": "as found_paths",
    "owner_aliases": "as found_paths",
    "trait_reachable": "reachability, as found_paths",
    "implementors": "the implementing types, which a new feature may extend (json adds StructArray)",
    "bucket": "the classifier's opinion, not the item",
    "rules": "the classifier's opinion",
    "decided_by": "the classifier's opinion",
}


def identity(c):
    return json.dumps({k: v for k, v in c.items() if k not in NOT_IDENTITY}, sort_keys=True)


def index(path):
    by_id, by_key = {}, {}
    for c in json.load(open(path))["callables"]:
        by_id.setdefault(identity(c), []).append(c)
        by_key[c["key"]] = c
    return by_id, by_key


def main():
    old_inv, new_inv, frozen_p, release_p = sys.argv[1:5]
    old_id, old_key = index(old_inv)
    new_id, _ = index(new_inv)

    def remap(key):
        c = old_key.get(key)
        if c is None:
            sys.exit(f"refusing: {key} is not in the old inventory")
        ident = identity(c)
        olds, news = old_id.get(ident, []), new_id.get(ident, [])
        if len(olds) != 1 or len(news) != 1:
            sys.exit(f"refusing: {key} ({c['canonical_path']}) has {len(olds)} old and {len(news)} new callables with its identity")
        return news[0]["key"], c["canonical_path"]

    frozen = json.load(open(frozen_p))
    moved = 0
    for row in frozen:
        new, _ = remap(row["key"])
        moved += new != row["key"]
        row["key"] = new
    json.dump(frozen, open(frozen_p, "w"), indent=1)
    open(frozen_p, "a").write("\n")

    text = open(release_p).read()
    lines = text.split("\n")
    rows = moved_rows = 0
    for i, line in enumerate(lines):
        m = re.fullmatch(r'key = "([^"]+)"', line)
        if not m:
            continue
        new, path = remap(m.group(1))
        # the row's own path, where it names one, must be the mapped callable's
        for follow in lines[i + 1:i + 4]:
            pm = re.fullmatch(r'path = "([^"]+)"', follow)
            if pm and pm.group(1) != path:
                sys.exit(f"refusing: release row {m.group(1)} names {pm.group(1)}, the key is {path}")
        rows += 1
        moved_rows += new != m.group(1)
        lines[i] = f'key = "{new}"'
    open(release_p, "w").write("\n".join(lines))
    print(f"frozen: {len(frozen)} bindings, {moved} keys translated; release: {rows} keyed rows, {moved_rows} translated")


if __name__ == "__main__":
    main()
