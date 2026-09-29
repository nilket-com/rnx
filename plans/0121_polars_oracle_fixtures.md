# rnx 0121: oracle fixtures for the untested generated bindings

Status: plan. Record 0120 is closed on `origin/main` at `badc7a8`. Claude and Codex agreed this cut before the plan (to-codex-060 and Codex's reply). It is a fixture-only record. The Arrow write side (builders, mutation, `legacy::array`, and the constructors 0120 refused) moves to record 0122.

The starting v2 scoreboard is 4,115 available and 2,750 value-tested of 6,421 applicable callables (42.8% value-tested).

## The pool, reproducible

`probes/0121/baseline.py` classifies every `fixture_failed` case in 0120's committed v2 results (`probes/0120/evidence/oracle-results-v2.json.gz`) by the fixture that failed. Its output is `probes/0121/baseline-v2.json`. 0.55.2 has none.

| fixture | cases | obstacle today |
|---|---:|---|
| `i128` | 102 | the i64 series fixture is unpacked as Int128 without a cast |
| `u128` | 100 | the same, as UInt128 |
| `f16` | 98 | the same, as Float16 |
| `decimal` | 20 | the same, as Decimal |
| `try_from_storage` | 18 | a map built from an i64 series ("not a Map dtype") |
| `ext` | 14 | an extension unpacked from an i64 series |
| `deserialize_json_from_str` | 8 | the generic string literal `"x"` is not JSON |
| `new` | 3 | the generic float literal 1.5 is outside a fraction's [0, 1] |
| **total** | **363** | |

Each of these bindings is generated but never value-tested.

## Scope

**1. Typed source fixtures that cast before unpacking:** `series_i128`, `series_u128`, `series_f16`, `series_decimal`. They follow the existing `series_i8`/`series_u16` pattern and feed their producers (`Series::i128`, and so on).
- Each has a null and a boundary value where the width allows: a value beyond i64, and an f16 subnormal.
- A fixture whose dtype does not exist at a pin (Decimal at 0.55.2) is emitted only where its producer is bound, so both adapters compile.

**2. Argument literals, a closed and cited table.** A parameter whose generic literal is outside its domain gets a listed literal on both sides: a JSON document for `deserialize_json_from_str`, and a fraction in [0, 1] for `new`.
- The table names (path, parameter, Rune literal, Rust literal, citation).
- It is validated fail-closed: an uncited row, a duplicate, or a row naming no generated binding's parameter refuses generation.

**3. Storage fixtures for map and extension, only where the pinned API has a safe public constructor.** Otherwise the cases stay `fixture_failed` and are named with the exact constructor obstacle; no fixture is invented.

**No binding changes.** Both surfaces' entries and bindings stay byte-identical, and the freeze is unchanged. Only the fixtures, the oracle cases and the oracle results move.

## Proof

- **Each repaired recipe.** A control shows the fixture builds the intended physical dtype with representative values (a null, and a boundary value where relevant), and that Rune and Rust agree.
- **Every moved oracle case is listed**, from its old status to its new one:
  - `fixture_failed` → `match` is the goal;
  - any new `mismatch` or `both_error` is a product finding, reported with its cause and not counted as a fixture success;
  - no `match` case may regress.
- **Every remaining `fixture_failed` row** is reported by family, with its exact constructor obstacle.
- **Surfaces.** The generator's surfaces at both pins are identical to 0120's in every entry (the check ignores only the inventory file name).
- **Suites and launch.** The usual suites run, debug last. Launch is not expected to move; fixtures are test-support only, and this is measured to confirm.

## Stop rules

- Any surface entry or binding moves: stop.
- A repaired fixture exposes a mismatch: report it as a finding. It is not "fixed" by changing the fixture.
- A fixture would need a constructor or ownership policy that belongs to 0122: leave the cases `fixture_failed`, named.
