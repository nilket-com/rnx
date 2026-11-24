#!/bin/sh
# Archive the specified commit, including the Git proof used by export-aware
# build scripts. Older revisions keep their original archive format.
set -eu
export GIT_OPTIONAL_LOCKS=0
rev=$(git rev-parse --verify "$1^{commit}")
if ! git cat-file -e "$rev:tools/export-source.sh" 2>/dev/null; then
	exec git archive "$rev"
fi
root=$(git rev-parse "$rev^{tree}")
rune=$(git rev-parse "$rev:rune")
proof=$(mktemp -d)
trap 'rm -rf -- "$proof"' EXIT
{
	printf '%s %s\n' "$rev" "$rune"
	git cat-file tree "$root"
} > "$proof/.rnx-export"
chmod 644 "$proof/.rnx-export"
git archive --mtime="@$(git show -s --format=%ct "$rev")" --add-file="$proof/.rnx-export" "$rev"
