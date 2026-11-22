#!/bin/sh
# Run a checked file under GDB against a debug image with Rune DWARF (-C debuginfo=2,
# opt-level=0). The launcher uses the same provider archive as the release control.
# Keep GDB separate from the ordinary runner so its diagnostics cannot mask a refusal.
set -eu
here=$(CDPATH='' cd -- "$(dirname -- "$0")" && pwd)
mode=$1
launcher=$2
shift 2
case $mode in
	checked-refusal) expected=2; allocations=0 ;;
	checked) expected=0; allocations=1 ;;
	*) exit 1 ;;
esac
# At this line admit() has charged the slot, and the native value becomes its owner.
source=$(realpath -- "$here/../../../rune/crates/rune/src/experimental_core/heap/host_value.rs")
line=$(awk '/^[[:space:]]*host\.value = Some\(value\.clone\(\)\);$/ {n++; line=NR} END {if(n!=1) exit 1; print line}' "$source")
commands=$(mktemp)
trap 'rm -f -- "$commands"' EXIT
cat >"$commands" <<'GDB'
set confirm off
set pagination off
set debuginfod enabled off
set auto-load off
set print thread-events off
set startup-with-shell off
set breakpoint pending off
break rnx_file_provider_open
run
set language c
set $next = (unsigned long long *)&'file_provider::NEXT'
# One token per owned Box<Value>. The sequence never rolls back on release.
set $first = *$next
set $charged = (unsigned long long)0
set $initial_charge = $charged
GDB
printf 'break -source "%s" -line %s\n' "$source" "$line" >>"$commands"
cat >>"$commands" <<'GDB'
commands
  silent
  set language rust
  set $charged = $charged + (*entry).charged_bytes
  continue
end
break rnx_file_provider_close
continue
finish
set language c
set $last = *$next
set $allocated = $last - $first
set $debits = $charged - $initial_charge
continue
if $_isvoid($_exitcode) || $_exitcode != $expected
  echo unexpected launcher exit\n
  quit 1
end
printf "token_sequence=%llu..%llu cumulative_host_charge=%llu..%llu\n", $first, $last, $initial_charge, $charged
printf "provider_allocations=%llu store_host_bytes_debited=%llu\n", $allocated, $debits
if $allocated != $allocations || (($debits > 0) != ($allocations > 0))
  quit 1
end
GDB
DEBUGINFOD_URLS= gdb -nx -q -batch \
	-ex "set \$expected = $expected" -ex "set \$allocations = $allocations" \
	-x "$commands" --args "$launcher" "$mode" "$@"
