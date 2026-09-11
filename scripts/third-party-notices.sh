#!/usr/bin/env bash
# Build THIRD-PARTY-NOTICES.md from the resolved dependency set and the
# licence texts those packages ship — plus, for packages that ship none, the
# verified copies under third-party/licenses/.
#
#   scripts/third-party-notices.sh           rewrite the file
#   scripts/third-party-notices.sh --check    fail if the file is out of date
#
# Deterministic: no dates, no paths from outside the repository, everything
# sorted. Offline: the network is only needed by fetch-missing-licenses.sh.
#
# **Every step that can fail, fails.** An earlier version suppressed Cargo's
# errors and ignored its exit status, so a dependency query that failed
# produced a file describing zero packages and `--check` then agreed with it.
# Nothing is written unless the whole run succeeds, so a failure leaves the
# existing notices in place.
set -euo pipefail
cd "$(dirname "$0")/.." || exit 2
export LC_ALL=C

cargo=${CARGO:-cargo}
targets=(x86_64-unknown-linux-gnu x86_64-pc-windows-msvc)
out=THIRD-PARTY-NOTICES.md
vendored=third-party/licenses
sources=$vendored/SOURCES.tsv
check=${1-}
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT

fail() {
	echo "third-party-notices: $*" >&2
	exit 2
}

"$cargo" metadata --locked --format-version 1 >"$work/meta.json" 2>"$work/err" ||
	fail "cargo metadata failed: $(head -3 "$work/err")"

# The scope: normal dependencies with procedural-macro subtrees pruned, per
# target, unioned. Each query's status is checked; a target that cannot be
# resolved stops the run rather than quietly contributing nothing.
: >"$work/union"
for target in "${targets[@]}"; do
	"$cargo" tree --locked --target "$target" -e normal,no-proc-macro \
		--prefix none --no-dedupe >"$work/tree" 2>"$work/err" ||
		fail "cargo tree failed for $target: $(head -3 "$work/err")"
	# Filtered with awk rather than a pipeline, so the step has one exit
	# status worth reading — `grep -v` answers 1 for "nothing matched", which
	# is indistinguishable from an error and was previously swallowed by
	# `|| true`.
	awk '{ sub(/ \(\*\)$/, ""); if ($0 != "" && $0 !~ /^rnx v0\.0\.0/) print }' \
		"$work/tree" >"$work/filtered" || fail "cannot read the tree for $target"
	# **This target's** output, not the union. Checking the union passes as
	# soon as one target has contributed, so a second target that answered
	# nothing at all went unnoticed and quietly dropped its platform's
	# packages.
	[ -s "$work/filtered" ] ||
		fail "cargo tree returned no packages for $target"
	cat "$work/filtered" >>"$work/union"
done
sort -u "$work/union" -o "$work/union"
[ -s "$work/union" ] || fail "the resolved dependency set is empty"

spdx_of() {
	jq -r --arg n "$1" --arg v "$2" \
		'.packages[] | select(.name==$n and .version==$v) | .license // "not declared"' \
		"$work/meta.json" | head -1
}

: >"$work/index"
: >"$work/textless"
: >"$work/provenance"
while read -r name version; do
	version=${version#v}
	manifest=$(jq -r --arg n "$name" --arg v "$version" \
		'.packages[] | select(.name==$n and .version==$v) | .manifest_path' "$work/meta.json" | head -1)
	[ -n "$manifest" ] || fail "no manifest recorded for $name $version"
	dir=$(dirname "$manifest")
	[ -d "$dir" ] || fail "$name $version: $dir is not a directory"
	spdx=$(spdx_of "$name" "$version")

	found=0
	while IFS= read -r file; do
		[ -f "$dir/$file" ] || continue
		found=1
		digest=$(sha256sum "$dir/$file" | cut -d' ' -f1) || fail "cannot digest $dir/$file"
		cp "$dir/$file" "$work/text.$digest" || fail "cannot read $dir/$file"
		printf '%s\t%s\t%s\t%s\t%s\n' "$digest" "$name" "$version" "$spdx" "$file" >>"$work/index"
	done < <(ls "$dir" | grep -iE '^(licen[cs]e|copying|notice|copyright)' | sort || true)

	# Shipped nothing: fall back to a verified copy, if one is checked in.
	if [ "$found" = 0 ] && [ -d "$vendored/$name-$version" ]; then
		while IFS= read -r file; do
			[ -f "$vendored/$name-$version/$file" ] || continue
			recorded=$(awk -F'\t' -v n="$name" -v v="$version" -v f="$file" \
				'$1==n && $2==v && $5==f {print $6"\t"$3"\t"$4}' "$sources")
			[ -n "$recorded" ] || fail "$name $version: $file is checked in but not in $sources"
			digest=$(sha256sum "$vendored/$name-$version/$file" | cut -d' ' -f1)
			want=$(printf '%s' "$recorded" | cut -f1)
			[ "$digest" = "$want" ] ||
				fail "$name $version: $file does not match the digest recorded in $sources"
			found=1
			cp "$vendored/$name-$version/$file" "$work/text.$digest"
			printf '%s\t%s\t%s\t%s\t%s\n' "$digest" "$name" "$version" "$spdx" "$file" >>"$work/index"
			printf '%s\t%s\t%s\t%s\t%s\n' "$name" "$version" "$file" \
				"$(printf '%s' "$recorded" | cut -f2)" "$(printf '%s' "$recorded" | cut -f3)" \
				>>"$work/provenance"
		done < <(ls "$vendored/$name-$version" | sort)
	fi

	[ "$found" = 1 ] || printf '%s\t%s\t%s\n' "$name" "$version" "$spdx" >>"$work/textless"
done <"$work/union"

crates=$(wc -l <"$work/union" | tr -d ' ')
texts=$(cut -f1 "$work/index" | sort -u | wc -l | tr -d ' ')
missing=$(wc -l <"$work/textless" | tr -d ' ')
fetched=$(wc -l <"$work/provenance" | tr -d ' ')
[ "$crates" -gt 0 ] || fail "no packages were resolved"
[ "$texts" -gt 0 ] || fail "no licence texts were found"

{
	cat <<'HEAD'
# Third-party notices

The packages rnx depends on, and the licence texts they ship. Generated by
`scripts/third-party-notices.sh` from the locked dependency set; do not edit by
hand.

## What this is, and what it is not

**These are the packages in rnx's selected dependency scope**, described
below. Their licences ask for their notices to travel with a distribution that
includes them, and this file is what travels with one. A release that ships a
binary without it is incomplete.

**A source distribution redistributes none of these packages.** The published
`.crate` and this repository hold rnx's own source; Cargo fetches each
dependency at build time, from its own registry, under its own terms. What
that does *not* establish is that a source distribution carries no
third-party material at all: record 0028 records three files in rnx written by
reading upstream Rune — the JSON pre-walk, the budget sentinel and the escape
decoder — and whether a mirrored interface warrants attribution is an open
question there, not one this file answers.

**The set is a selected scope, not an inventory of a binary.** It is what
Cargo's graph can answer: normal dependency edges with procedural-macro
subtrees pruned, one query per target, unioned across Linux and Windows. It is
not the linker's account of what an artifact retains. Nor is the pruning a
claim that nothing from those packages reaches the artifact — a derive macro
emits code that is then compiled in, so excluding `serde_derive` and its kind
excludes *packages that are linked*, not *material that appears*. Build-time
packages are excluded on the same basis and with the same caveat; one of them,
`cfg_aliases`, reached through `nix` on unix targets, ships `NOTICES.md`
carrying MIT terms for code taken from `tectonic_cfg_support`.

Which of those unresolved questions bear on a particular distribution is a
review of that distribution. This file is an input to it.

HEAD
	printf '## The packages\n\n%s packages, %s distinct licence texts.\n\n' "$crates" "$texts"
	printf '| Package | Version | Declared |\n| --- | --- | --- |\n'
	while read -r name version; do
		version=${version#v}
		printf '| `%s` | %s | %s |\n' "$name" "$version" "$(spdx_of "$name" "$version")"
	done <"$work/union"

	if [ "$fetched" != 0 ]; then
		cat <<'FETCHED'

## Texts that the package did not ship

These packages declare a licence and include no copy of it in what they
publish. The text below each of them is **not from the package**: it is the
file that stood in the project at the exact revision the package was published
from, which `.cargo_vcs_info.json` records. That is a weaker claim than a
bundled text and is made explicitly rather than silently.

The copies are checked in under `third-party/licenses/`, with their revision,
path and digest in `SOURCES.tsv`, so the generator needs no network and a
reader can repeat the fetch. `scripts/fetch-missing-licenses.sh` is what
produced them.

FETCHED
		printf '| Package | File | Revision | Path |\n| --- | --- | --- | --- |\n'
		sort "$work/provenance" | while IFS=$'\t' read -r name version file sha path; do
			printf '| `%s %s` | %s | `%s` | `%s` |\n' "$name" "$version" "$file" "$sha" "$path"
		done
	fi

	if [ "$missing" != 0 ]; then
		cat <<'GAP'

## Packages with no licence text anywhere

Declared in metadata, absent from the package, and absent from the project at
the revision published. Their declaration is reproduced above; there is no
text to reproduce, from either source.

That is a gap rather than a formality. MIT asks for "the above copyright
notice" to be included, and no such notice exists to include — inventing one
would mean naming a copyright holder nobody has stated. What remains is an
upstream conversation with the project, which is a decision rather than a
build step.

GAP
		printf '| Package | Version | Declared |\n| --- | --- | --- |\n'
		while IFS=$'\t' read -r name version spdx; do
			printf '| `%s` | %s | %s |\n' "$name" "$version" "$spdx"
		done <"$work/textless"
	fi

	printf '\n## The texts\n'
	for digest in $(cut -f1 "$work/index" | sort -u); do
		printf '\n### '
		grep "^$digest	" "$work/index" | while IFS=$'\t' read -r _ name version _ file; do
			printf '`%s %s` (%s) ' "$name" "$version" "$file"
		done
		printf '\n\n```\n'
		cat "$work/text.$digest"
		printf '```\n'
	done
} >"$work/notices.md"

if [ "$check" = "--check" ]; then
	if diff -q "$out" "$work/notices.md" >/dev/null 2>&1; then
		echo "$out is up to date: $crates packages, $texts texts, $fetched fetched, $missing unresolved"
		exit 0
	fi
	echo "$out is out of date; regenerate it with scripts/third-party-notices.sh" >&2
	diff -u "$out" "$work/notices.md" | head -40 >&2
	exit 1
fi

cp "$work/notices.md" "$out"
echo "wrote $out: $crates packages, $texts texts, $fetched fetched, $missing unresolved"
