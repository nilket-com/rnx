#!/usr/bin/env python3
"""Record 0122: the pre-fix adapter source for the before table. It is the
plan commit's lib.rs (b7d2c17, with record 0058's collect dtype check) plus
only this record's test-support `oracle_repr` and its registration, taken
from the current lib.rs.

  before_lib.py <plan-commit lib.rs> <current lib.rs> <out lib.rs>
"""
import sys

before, current, out = open(sys.argv[1]).read(), open(sys.argv[2]).read(), sys.argv[3]
start = current.index("/// Record 0122 (test support)")
end = current.index("/// Install into an rnx-created `polars` module.")
anchor = "/// Install into an rnx-created `polars` module."
assert "files::validate(&frame)?;" in before, "the plan commit's collect check is missing"
assert "oracle_repr" not in before
before = before.replace(anchor, current[start:end] + anchor, 1)
reg = '''	#[cfg(all(feature = "generated", feature = "test-support"))]
	m.function("oracle_repr", oracle_repr).build().map_err(err)?;
'''
hook = '''	#[cfg(feature = "test-support")]
	m.function("engine_counts", engine::counts)'''
assert before.count(hook) == 1
before = before.replace(hook, reg + hook, 1)
open(out, "w").write(before)
