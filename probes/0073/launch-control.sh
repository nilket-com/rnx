#!/usr/bin/env bash
# Negative control: the launch driver must fail on a binary that fails.
set -uo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
if python3 "$here/launch.py" failing=/bin/false --runs 2 --out /dev/null > /dev/null 2>&1; then
  echo "CONTROL FAIL: launch.py accepted a failing binary"; exit 1
fi
echo "control ok: a failing binary fails launch.py"
