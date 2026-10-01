#!/bin/sh
# Record 0133: the session gate fails closed. A run that returns Err, and a
# script that doesn't compile, must each make session.py exit non-zero; a run
# that succeeds must pass. Plain sessions (no :dep needed).
set -u
here=$(cd "$(dirname "$0")" && pwd)
rnx=${1:?usage: session_controls.sh RNX}
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
fail=0
printf 'pub fn run(args) {\n    println!("WORKFLOW OK ok");\n    Ok(())\n}\n' > "$work/ok.rn"
printf 'pub fn run(args) {\n    Err("boom")\n}\n' > "$work/err.rn"
printf 'pub fn run(args) {\n    let x = ;\n    println!("WORKFLOW OK broken");\n    Ok(())\n}\n' > "$work/broken.rn"
printf 'pub fn run(args) {\n    println!("WORKFLOW OK early");\n    let v = [];\n    v[3]\n}\n' > "$work/late.rn"
printf 'pub fn run(args) {\n    println!("WORKFLOW OK early");\n    Err("boom")\n}\n' > "$work/marker_err.rn"
check() { # name, script, expected status (0 pass, 1 fail)
	RNX_SESSION_ARGS=repl python3 "$here/session.py" "$rnx" "$work/$2" "$work/$2.log" >/dev/null 2>&1
	got=$?
	[ "$got" -eq "$3" ] && echo "ok: $1" || { echo "FAIL: $1 (status $got)"; fail=1; }
}
check "a successful run passes" ok.rn 0
check "a run returning Err fails" err.rn 1
check "a script that doesn't compile fails" broken.rn 1
check "a runtime error after the marker fails" late.rn 1
check "a returned Err after the marker fails" marker_err.rn 1
exit $fail
