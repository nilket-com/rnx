#!/usr/bin/env python3
"""Record 0079 gate 2: the report from out/contract.json and out/subprocess.json."""
import json, sys, os
out = sys.argv[1]
c = json.load(open(os.path.join(out, "contract.json")))
s = json.load(open(os.path.join(out, "subprocess.json")))
def m(x): return x["median"]
def rng(x): return f"{x['min']:.3f}..{x['max']:.3f}"
print("# Record 0079 gate 2: contract probe\n")
print("## Costs (release build; medians with min..max over interleaved samples; instrumentation off)\n")
print("| measurement | Rune ms | Rust ms | note |\n|---|---:|---:|---|")
for n in (1, 1000, 1000000):
    k = c["costs"][f"apply_mut_{n}"]
    print(f"| apply_mut, {n} calls, in place, end to end | {m(k['rune_in_place_ms']):.3f} ({rng(k['rune_in_place_ms'])}) | {m(k['rust_closure_ms']):.4f} | {k['note']} |")
    print(f"| apply_mut, {n} calls, commit on success | {m(k['rune_commit_on_success_ms']):.3f} ({rng(k['rune_commit_on_success_ms'])}) | {m(k['rust_clone_then_apply_ms']):.4f} | paired commit minus in place: median {m(k['commit_minus_in_place_ms_paired']):.3f} ms ({rng(k['commit_minus_in_place_ms_paired'])}) |")
b = c["costs"]["bridge_alone_single_thread"]
print(f"| bridge alone, one thread, {b['calls']} calls, i64 in and out | {m(b['scalar_i64_in_out_ms']):.2f} ({rng(b['scalar_i64_in_out_ms'])}) | | {b['per_call_us_scalar']:.3f} us per call: one VM per call |")
print(f"| bridge alone, one thread, {b['calls']} calls, Series in and out | {m(b['series_in_out_ms']):.2f} ({rng(b['series_in_out_ms'])}) | | {b['per_call_us_series']:.3f} us per call; conversion over scalar {b['per_call_us_series_conversion_over_scalar']:.3f} us |")
k = c["costs"]["apply_columns_par_1000_one_row_columns"]
print(f"| apply_columns_par, 1000 one-row columns, parallel wall time | {m(k['rune_ms']):.3f} ({rng(k['rune_ms'])}) | {m(k['rust_ms']):.3f} | {k['note']} |")
k = c["costs"]["elementwise_series_1m_rows"]
print(f"| apply_unary_elementwise, one call on 1 000 000 rows | {m(k['rune_ms']):.3f} ({rng(k['rune_ms'])}) | {m(k['rust_ms']):.3f} | {k['note']} |")
bc = c["budget_cost"]
print(f"| budget wrapper, {bc['calls']} calls, paired difference | | | median {bc['paired_diff_ns_median']:.1f} ns per call ({bc['paired_diff_ns_min']:.1f}..{bc['paired_diff_ns_max']:.1f}) |")
print("\n## Concurrency\n")
print(json.dumps(c["concurrency"]))
print("\n## Deferred journey\n")
for k, v in c["deferred_journey"].items():
    print(f"- {k}: {v}")
print("\n## CSV, budget discrimination, restoration, re-entry, mutation, errors, captures, budget\n")
for sec in ("csv", "budget_discrimination", "restoration", "reentry", "mutation", "errors", "captures", "budget"):
    for k, v in c[sec].items():
        print(f"- {sec}.{k}: {v}")
print("\n## Subprocess controls (watchdog; survivors are group members alive after exit)\n")
for k, v in s.items():
    print(f"- {k}: {v['verdict']} (expected {v['expected']}, {v['status']}, survivors {v['survivors']}) {json.dumps(v['output'])[:200]}")
print("\nfailures:", c["failures"])
