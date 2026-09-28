#!/usr/bin/env bash
# Record 0108: the isolated Python environment for the census and the
# differential oracle. Hash-pinned, no dependencies beyond the two wheels.
#
#   python.sh            # creates target/0108/pyenv once, then verifies it; prints the interpreter path
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"; root="$(cd "$here/../.." && pwd)"
env="$root/target/0108/pyenv"
if [ ! -x "$env/bin/python" ]; then
  python3 -m venv "$env" >&2
  "$env/bin/pip" install -q --no-deps --require-hashes -r "$here/py-requirements.txt" >&2
fi
"$env/bin/python" - <<'PY' >&2
import polars, polars._plr as plr
assert polars.__version__ == "2.0.0-rc.2", polars.__version__
assert plr._BUILD_COMMIT == "da47b7405f0e2d188e71cce7c2d588c695f4098d", plr._BUILD_COMMIT
PY
echo "$env/bin/python"
