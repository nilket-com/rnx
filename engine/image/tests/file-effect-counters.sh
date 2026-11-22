#!/bin/sh
# The release observer reports cumulative deltas from before open through close.
set -eu
mode=$1
launcher=$2
shift 2
log=$(mktemp)
trap 'rm -f -- "$log"' EXIT
status=0
"$launcher" "$mode" "$@" >/dev/null 2>"$log" || status=$?
case "$mode:$status" in
	checked:0) expected='provider_allocations=1 store_host_bytes_debited=[1-9][0-9]*' ;;
	checked-refusal:2) expected='provider_allocations=0 store_host_bytes_debited=0' ;;
	*) cat "$log" >&2; exit 1 ;;
esac
test "$(grep -c '^provider_allocations=' "$log")" -eq 1
row=$(grep '^provider_allocations=' "$log")
matches() { printf '%s\n' "$1" | grep -Exq "$expected"; }
matches "$row"
if [ "$mode" = checked-refusal ]; then
	# Independently plant one allocation and one byte in the observed row.
	if matches "provider_allocations=1 ${row#* }" || matches "${row%% *} store_host_bytes_debited=1"; then exit 1; fi
fi
printf '%s\n' "$row"
