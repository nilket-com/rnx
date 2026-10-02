#!/bin/sh
# Record 0150: each reproduction pasted into a real session (0133's driver).
#   paste.sh RNX WORK   (RNX_DEP unset: the runner's repl; set: :dep first)
rnx=$1; work=$2; here=$(dirname "$0")
mkdir -p "$work"
for f in for if while block const; do
	if [ -n "$RNX_DEP" ]; then extra=""; else extra="repl"; fi
	r=$(RNX_SESSION_ARGS=$extra timeout 3000 python3 "$here/../0133/session.py" "$rnx" "$here/$f.rn" "$work/$f.log" x | head -1)
	echo "$f: $r"
done
