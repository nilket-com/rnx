#!/usr/bin/env bash
# Fetch the licence texts that packages declare but do not ship, from the exact
# revision each package was published from, and check them in.
#
# Needs the network; nothing else does. `third-party/licenses/` is consumed
# offline by scripts/third-party-notices.sh, and SOURCES.tsv records where every
# file came from so a reader can repeat the fetch.
#
# A package's own tarball is the first authority. This is only for packages that
# ship no text at all, and it is not a substitute for one: what it fetches is the
# text that stood in the project at the revision published, which is a different
# claim and is recorded as one.
#
# **A failed request is not an absent file.** An earlier version treated every
# non-200 the same, so a run against a server answering 503 reported "no licence
# found" for everything and replaced SOURCES.tsv with its header alone —
# converting an outage into the claim that nothing exists. Only 404 means a
# candidate path is absent; anything else stops the run, and the previous set
# stays where it is.
#
# `RNX_TEST_PUBLICATION_FAILS` makes the switch fail after a successful fetch,
# which is the one failure a stub server cannot produce and the one that used
# to destroy everything. `RNX_TEST_ROLLBACK_FAILS` then fails the rollback, so
# a gate can leave the licences in the recovery directory and run the script
# again — the retry that used to delete them.
set -euo pipefail
cd "$(dirname "$0")/.." || exit 2
export LC_ALL=C

cargo=${CARGO:-cargo}
out=third-party/licenses
sources=$out/SOURCES.tsv
work=$(mktemp -d)
staging=$work/staging
mkdir -p "$staging"
trap 'rm -rf "$work"' EXIT

fail() {
	echo "fetch-missing-licenses: $*" >&2
	echo "nothing was changed; $out is as it was" >&2
	exit 2
}

"$cargo" metadata --locked --format-version 1 >"$work/meta.json" 2>"$work/err" ||
	fail "cargo metadata failed: $(head -3 "$work/err")"

candidates=(LICENSE-MIT LICENSE-APACHE LICENSE LICENSE.md LICENSE.txt COPYING COPYRIGHT)

printf 'package\tversion\trevision\tpath\tfile\tsha256\turl\n' >"$staging/sources"
searched=0
while IFS=$'\t' read -r name version; do
	[ -n "$name" ] || continue
	searched=$((searched + 1))
	manifest=$(jq -r --arg n "$name" --arg v "$version" \
		'.packages[] | select(.name==$n and .version==$v) | .manifest_path' "$work/meta.json" | head -1)
	[ -n "$manifest" ] || fail "no manifest recorded for $name $version"
	dir=$(dirname "$manifest")
	info=$dir/.cargo_vcs_info.json
	repo=$(jq -r --arg n "$name" --arg v "$version" \
		'.packages[] | select(.name==$n and .version==$v) | .repository // ""' "$work/meta.json" | head -1)
	if [ ! -f "$info" ] || [ -z "$repo" ]; then
		echo "  $name $version: no published revision or repository recorded; not searched" >&2
		continue
	fi
	sha=$(jq -r '.git.sha1 // ""' "$info")
	[ -n "$sha" ] || fail "$name $version: $info records no revision"
	in_vcs=$(jq -r '.path_in_vcs // ""' "$info")
	slug=${repo#https://github.com/}
	slug=${slug%.git}
	if [ "$slug" = "$repo" ]; then
		echo "  $name $version: $repo is not a GitHub repository; not searched" >&2
		continue
	fi

	got=0
	for file in "${candidates[@]}"; do
		for path in "$file" ${in_vcs:+"$in_vcs/$file"}; do
			url="https://raw.githubusercontent.com/$slug/$sha/$path"
			code=$(curl -sS --max-time 30 -o "$work/body" -w '%{http_code}' "$url" 2>"$work/curlerr") ||
				fail "$url: the request failed: $(head -1 "$work/curlerr")"
			case "$code" in
			200) ;;
			404)
				# The one answer that means the file is not there.
				continue
				;;
			*)
				fail "$url: answered $code, which says nothing about whether the file exists"
				;;
			esac
			mkdir -p "$staging/$name-$version"
			cp "$work/body" "$staging/$name-$version/$file"
			digest=$(sha256sum "$staging/$name-$version/$file" | cut -d' ' -f1)
			printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\n' \
				"$name" "$version" "$sha" "$path" "$file" "$digest" "$url" >>"$staging/sources"
			echo "  $name $version: $file from $sha:$path"
			got=$((got + 1))
			break
		done
	done
	[ "$got" != 0 ] ||
		echo "  $name $version: every candidate path answered 404 at $sha; no text exists there" >&2
done

[ "$searched" != 0 ] || fail "no packages were given to search for"

# Published only now, and as a whole. The replacement is assembled **beside**
# the destination and switched in; the old directory is kept until the switch
# has succeeded and put back if it has not. Removing the destination first and
# copying into it — which this did — means any failure during the copy leaves
# nothing at all, and a fetcher that deletes the licences it is meant to keep
# is worse than one that never ran.
{
	head -1 "$staging/sources"
	tail -n +2 "$staging/sources" | sort
} >"$staging/sources.sorted"

incoming=$out.incoming
previous=$out.previous

# A recovery copy left by an earlier run is the only remaining copy of the
# licences, and removing it here — which this did, unconditionally — turns a
# recoverable failure into a total loss on the next attempt. So it is either
# put back or refused, never deleted.
if [ -d "$previous" ]; then
	if [ -d "$out" ]; then
		fail "$previous is a recovery copy from an earlier run, and $out exists too;\nmove or remove $previous once you have decided which of the two is right"
	fi
	mv "$previous" "$out" || fail "cannot recover $previous into $out"
	echo "recovered the previous licences from $previous" >&2
fi
rm -rf "$incoming"
mkdir -p "$incoming"
for staged in "$staging"/*/; do
	[ -d "$staged" ] || continue
	cp -r "$staged" "$incoming/" || fail "cannot assemble the replacement in $incoming"
done
cp "$staging/sources.sorted" "$incoming/$(basename "$sources")" ||
	fail "cannot assemble the replacement record"

# The switch. Everything below leaves the previous set reachable until the
# new one is in place.
if [ -d "$out" ]; then
	mv "$out" "$previous" || fail "cannot move the previous licences aside"
fi
if [ -n "${RNX_TEST_PUBLICATION_FAILS:-}" ] || ! mv "$incoming" "$out"; then
	# Put the previous set back before reporting. A failed switch that left
	# the destination missing would be the same loss by another route.
	if [ -d "$previous" ] && { [ -n "${RNX_TEST_ROLLBACK_FAILS:-}" ] || ! mv "$previous" "$out"; }; then
		echo "fetch-missing-licenses: the previous licences could not be put back;" >&2
		echo "they are at $previous" >&2
		exit 2
	fi
	rm -rf "$incoming"
	fail "the replacement could not be published; the previous licences are unchanged"
fi
rm -rf "$previous"
echo "recorded $(($(wc -l <"$sources") - 1)) files from $searched packages in $sources"
