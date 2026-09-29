#!/usr/bin/env bash
# Record 0115: each mutation must be caught by the proof named for it.
#   mutations.sh <32d1b40 polars-gen> <scratch dir>
set -u
export LC_ALL=C
old=$(realpath "$1"); s=$2; mkdir -p "$s"
here=$(cd "$(dirname "$0")" && pwd); root=$(cd "$here/../.." && pwd)
gen="$root/tools/polars-gen"; t="$root/target/0115/mutation"
build() { CARGO_TARGET_DIR="$t" cargo build -q --release --locked --manifest-path "$gen/Cargo.toml" 2>/dev/null; }
tests() { CARGO_TARGET_DIR="$t" cargo test -q --release --locked --manifest-path "$gen/Cargo.toml" "$@" > "$s/test.log" 2>&1; }
mutate() { # file, python replacement (old, new)
  cp "$gen/src/$1" "$s/backup.rs"
  python3 - "$gen/src/$1" "$2" "$3" <<'PY'
import sys; p, a, b = sys.argv[1:4]; t = open(p).read(); assert t.count(a) == 1, a; open(p, 'w').write(t.replace(a, b))
PY
}
restore() { cp "$s/backup.rs" "$gen/src/$1"; }
result() { if [ "$2" = caught ]; then echo "caught: $1"; else echo "NOT CAUGHT: $1"; fail=1; fi; }
fail=0
# 1. two families swapped in one hook's order: the pinned order test fails
mutate families/mod.rs $'pub(crate) static RET_SCALAR: &[&dyn Family] = &[&HASH_TOKEN, &BOUNDED_READBACK];' $'pub(crate) static RET_SCALAR: &[&dyn Family] = &[&BOUNDED_READBACK, &HASH_TOKEN];'
tests hook_orders && r=missed || r=caught; restore families/mod.rs; result "RET_SCALAR order swapped (hook_orders_are_pinned)" $r
# 2. the scope's clear removed: the scope and no-leak tests fail
mutate families/mod.rs $'		for f in self.scope {\n			m.remove(f.name());\n		}\n	}\n}' $'		let _ = (&mut m, self.scope);\n	}\n}'
tests && r=missed || r=caught; restore families/mod.rs; result "scope clear removed (scopes_replace_and_clear, cow-return no-leak)" $r
# 3. one family's ret arm dropped: golden differs
mutate families/mod.rs $'	&ARRAY,\n	&INDEXED,\n	&CHUNK,\n	&NULL_AWARE,\n	&ITERATOR_RETURN,\n];' $'	&INDEXED,\n	&CHUNK,\n	&NULL_AWARE,\n	&ITERATOR_RETURN,\n];'
build; "$root/probes/0114/golden.sh" "$t/release/polars-gen" "$s/g-mut" > /dev/null; "$root/probes/0114/golden.sh" "$old" "$s/g-old" > /dev/null
diff -rq "$s/g-old" "$s/g-mut" > /dev/null && r=missed || r=caught; restore families/mod.rs; result "array ret arm dropped from RET_TOP (golden)" $r
# 4. `collect` made first-match: the overlap comparison fails
mutate families/mod.rs $'				states.push((f.name(), s));\n' $'				states.push((f.name(), s));\n				return Ok(states);\n'
build; "$here/overlap.sh" "$old" "$t/release/polars-gen" "$s/ov-mut" > "$s/ov.log" 2>&1 && r=missed || r=caught; restore families/mod.rs; result "listed made first-match (overlap)" $r
# 5. the unknown-key check removed: the typo control loads
mutate release.rs $'			Some(k) => Err(k.clone()),' $'			Some(_) => Ok(()),'
tests unknown_release_keys && r=missed || r=caught; restore release.rs; result "unknown-key check removed (unknown_release_keys)" $r
# 6. two claims swapped: the pinned order test fails
mutate families/mod.rs $'	&FROM_ITER,\n	&UNARY,\n' $'	&UNARY,\n	&FROM_ITER,\n'
tests hook_orders && r=missed || r=caught; restore families/mod.rs; result "CLAIM order swapped (hook_orders_are_pinned)" $r
# 7. the preflights placed in list order instead of before the earlier ones
mutate families/mod.rs $'					preflight.insert_str(0, &p);' $'					preflight.push_str(&p);'
tests checks_run_all && r=missed || r=caught; restore families/mod.rs; result "preflight order reversed (checks_run_all_until_a_refusal)" $r
# 8. a ret collision resolved by the later family: the first-answer control fails
mutate families/mod.rs $'	&ARRAY,\n	&INDEXED,\n	&CHUNK,\n	&NULL_AWARE,' $'	&INDEXED,\n	&ARRAY,\n	&CHUNK,\n	&NULL_AWARE,'
tests ret_first_answer && r=missed || r=caught; restore families/mod.rs; result "RET_TOP array/indexed swapped (ret_first_answer_wins)" $r
build
[ $fail = 0 ] && echo "mutations: all caught" || { echo "mutations: FAILED"; exit 1; }
