#!/bin/sh
# Paths: linked image, empty private output directory, optional stock oracle.
set -eu
src=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
image=$(realpath -- "$1")
out=$(realpath -- "$2")
stock=${3:-}
cc -std=c11 -O2 -Wall -Wextra -Werror "$src/stock.c" "$image" "-Wl,-rpath,$(dirname -- "$image")" -o "$out/invoke"
cd "$out"
cat >helper.rn <<'RUNE'
pub fn value(a) { [path::join("a", "b"), a, env::args()] }
RUNE
cat >ordinary.rn <<'RUNE'
mod helper;
pub fn main(a) { helper::value(a) }
RUNE
cat >terminal.rn <<'RUNE'
pub fn main(a) { fs::append(a[0], "x")?; panic!("{}", a[1]); }
RUNE
cat >refused.rn <<'RUNE'
pub fn main(a) { fs::append(a[0], "x")?; b"bytes" }
RUNE
for code in 0 23; do
	cat >"exit-$code.rn" <<RUNE
pub fn main(_) {
	println!("leaving");
	print!("stdout tail");
	io::eprint("stderr tail")?;
	process::exit($code)?;
	println!("returned from exit");
}
RUNE
done
printf 'leaving\nstdout tail' >expected-exit.out
printf 'rnx: engine=execution native-requested=on\nstderr tail' >expected-exit.err
cat >interrupt.rn <<'RUNE'
pub async fn main(_) { println!("ready"); loop {} }
RUNE
run() {
	label=$1; expected=$2; shift 2
	status=0
	"$@" >"$label.out" 2>"$label.err" || status=$?
	test "$status" = "$expected"
}
long=$(printf '%09000d' 0)
for role in image stock; do
	if [ "$role" = image ]; then set -- ./invoke
	elif [ -n "$stock" ]; then set -- "$stock" run --debug-engine --budget 2000000000
	else continue
	fi
	run "$role-normal" 0 "$@" ordinary.rn 'two words' ''
	test "$(cat "$role-normal.out")" = '["a/b", ["two words", ""], ["two words", ""]]'
	test "$(cat "$role-normal.err")" = 'rnx: engine=execution native-requested=on'
	run "$role-terminal" 1 "$@" terminal.rn "$role-count" "$long"
	test "$(cat "$role-count")" = x
	test ! -s "$role-terminal.out"
	grep -Fq "Panicked: $long" "$role-terminal.err"
	for code in 0 23; do
		run "$role-exit-$code" "$code" env RNX_TEST_MUST_EXIT=1 "$@" "exit-$code.rn"
		cmp expected-exit.out "$role-exit-$code.out"
		cmp expected-exit.err "$role-exit-$code.err"
	done
	# Signal after the file's first effect. Bound the entire handshake.
	mkfifo "$role.pipe"
	timeout --kill-after=2 15 sh -c '
		role=$1; shift
		"$@" interrupt.rn >"$role.pipe" 2>"$role-interrupt.err" &
		pid=$!
		exec 3<"$role.pipe"
		IFS= read -r line <&3 && test "$line" = ready || exit 1
		printf "%s\n" "$line" >"$role-interrupt.out"
		kill -INT "$pid"
		status=0
		wait "$pid" || status=$?
		test "$status" = 130
	' sh "$role" "$@"
	grep -Fxq interrupted "$role-interrupt.err"
done
if [ -n "$stock" ]; then
	for name in normal terminal exit-0 exit-23 interrupt; do
		for stream in out err; do cmp "stock-$name.$stream" "image-$name.$stream"; done
	done
fi
run refused 78 ./invoke refused.rn refused-count
test ! -e refused-count
test ! -s refused.out
grep -q '^execution refused:' refused.err
