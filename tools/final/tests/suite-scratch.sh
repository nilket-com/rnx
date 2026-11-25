#!/usr/bin/env bash
# Pre-sign control. Keep the suite's normal mount namespace for its nested
# sandbox tests. Record its filesystem changes so a shared-directory delta
# can be traced to a child instead of being attributed by timing alone.
set -euo pipefail
export LC_ALL=C
evidence=$(realpath -m "${1:?supply a fresh evidence directory}")
shift
mkdir -- "$evidence"
snapshot() {
	find /var/tmp -mindepth 1 -maxdepth 1 -printf '%i\t%p\0' | sort -z
}
snapshot >"$evidence/before"
status=0
strace -f -qq --seccomp-bpf -yy -s 512 -e trace=mkdir,mkdirat,rename,renameat,renameat2,rmdir,unlink,unlinkat,link,linkat,symlink,symlinkat,creat,openat \
	-o "$evidence/files.trace" cargo +1.98.1 test --locked -p rnx-final "$@" || status=$?
snapshot >"$evidence/after"
if ! cmp -s "$evidence/before" "$evidence/after"; then
	for column in -23 -13; do
		comm -z "$column" "$evidence/before" "$evidence/after" |
			while IFS= read -r -d '' entry; do
				path=${entry#*$'\t'}
				printf '%s %q\n' "$column" "$entry"
				grep -F -- "$path" "$evidence/files.trace" || echo 'no matching suite path in trace'
			done
	done >&2
	exit 1
fi
exit "$status"
