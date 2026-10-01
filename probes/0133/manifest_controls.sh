#!/bin/sh
# Record 0133: the dataset gate fails closed. Each control copies the retained
# data, corrupts one thing, and requires `manifest.py check` to refuse it.
set -u
here=$(cd "$(dirname "$0")" && pwd)
data=${RNX_UAT_DATA:?set RNX_UAT_DATA}
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
fail=0
expect() { # name, then the corruption as python over the issues list `v`
	rm -rf "$work/data"; mkdir -p "$work/data/d2"; ln -s "$data/d1" "$work/data/d1"
	python3 - "$data/d2/issues.json" "$work/data/d2/issues.json" "$2" <<'PY'
import json, sys
v = json.load(open(sys.argv[1]))
exec(sys.argv[3])
json.dump(v, open(sys.argv[2], "w"))
PY
	if python3 "$here/manifest.py" check "$work/data" >/dev/null; then
		echo "FAIL: $1 was accepted"; fail=1
	else
		echo "ok: $1 refused"
	fi
}
python3 "$here/manifest.py" check "$data" >/dev/null && echo "ok: the retained data verifies" || { echo "FAIL: the retained data"; fail=1; }
expect "a duplicated issue" 'v.append(dict(v[0]))'
expect "a missing issue" 'v.pop(17)'
expect "changed content" 'v[100]["body"] += " edited"'
expect "a changed label" 'v[5]["labels"] = v[5]["labels"] + ["new"]'
expect "reordering" 'v[0], v[1] = v[1], v[0]'
rm -rf "$work/data"; mkdir -p "$work/data"; cp -r "$data/d2" "$work/data/d2"
python3 "$here/manifest.py" check "$work/data" >/dev/null && { echo "FAIL: missing D1 accepted"; fail=1; } || echo "ok: missing D1 refused"
rm -rf "$work/data"; mkdir -p "$work/data"; ln -s "$data/d1" "$work/data/d1"
python3 "$here/manifest.py" check "$work/data" >/dev/null && { echo "FAIL: missing D2 accepted"; fail=1; } || echo "ok: missing D2 refused"
exit $fail
