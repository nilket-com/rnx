#!/usr/bin/env bash
# Negative control: with a cargo that fails `check`, adjacent.sh must exit
# nonzero. Everything else (the generator build) runs the real cargo.
set -uo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
shim="$(mktemp -d)/cargo"
cat > "$shim" <<'SH'
#!/usr/bin/env bash
if [ "${1:-}" = check ]; then echo "error: injected failure" >&2; exit 7; fi
exec cargo "$@"
SH
chmod +x "$shim"
if ADJACENT_CARGO="$shim" ADJACENT_SCRATCH="$(mktemp -d)/adjacent" "$here/adjacent.sh" > /dev/null 2>&1; then
  echo "CONTROL FAIL: adjacent.sh reported success with a failing cargo check"; exit 1
fi
echo "control ok: a failing cargo check fails adjacent.sh"
