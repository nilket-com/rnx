# 0051 evidence: one entry point, an external executable

Measured by Codex on nano, Linux, 2026-09-15, Rust 1.98.1, Rune 0.14.2.
Plan commit: `d7e43a2`; before code: `b775a40`. Reproducer and raw exports:
rnx-bench commit `e506767`, `probes/extensions/` and
`results/extensions-0051/`. Nothing was installed in a user kernelspec directory.

## What changed

`src/lib.rs` contains the previous main, private modules and counting global
allocator. `src/main.rs` only calls `rnx::main_with(Extensions::none())` and
returns its result. Serving contexts install extensions after all batteries,
including run's argument snapshot. The worker uses that same context. Reset
retains it. Selfcheck and fast answers do not invoke builders, and settings
remain in their separate bare evaluator. The root manifest, lockfile, notices
and entire kernel package are unchanged.

`src/extensions.rs` stores ordered, non-Send FnOnce builders. It checks names
before calling each builder, checks returned help paths, and attributes
builder/install failures to the declared name. A scoped panic hook suppresses
only the calling thread and delegates others to the prior hook. Only unwinds
through the call are converted. Native adapters remain trusted; there is no
namespace confinement or sandbox claim.

## Gates and observations

1. **Stock behavior and startup.** `conditions.json` records all output and
   statuses from 18 single-file/CLI-error cases, plus the four timed commands,
   against before, stock-after and the external app. Every comparison is
   byte-identical. The extra CLI cases exercise ordinary Rust Termination
   errors as well as explicit exits. `timings.json` contains all samples.
2. **Every entry point.** `assembly.json` records run, eval, an async call,
   session help before/after reset, and the worker before/after reset. Stock
   rnx refuses the extension. `completion.ansi` and `completion.json` record
   real PTY tab completion of answer before reset and later after reset.
   A file propagating `fixture::fail()?` reports `error: fixture failure`;
   eval returning the Result does too. The known top-level `?` session-wrapper
   error remains `Expected type Tuple but found Result`, recorded separately,
   not changed or used as the ordinary-error gate.
3. **Startup refusals.** Empty/invalid/reserved/duplicate names, wrong help
   prefix, builder error, install collision and panic all exit 1 with exactly
   one named diagnostic and no splash/prompt. A valid inherited worker control
   transport receives no ready frame on builder failure. The other-thread
   panic case emits its original hook message while the app continues.
   `hook-restored.json` proves restoration after a successful builder: a
   post-main_with panic invokes the application's previous hook, once.
4. **Ordering.** The marker file stays absent for version, help and selfcheck;
   eval writes one line. Invalid settings referencing the extension report
   their error before the builder marker; the session then calls the function,
   shows its help, resets and calls it again with only one marker. A config
   unit test independently establishes that the serving context's extension
   is absent from the settings evaluator.
4b. **Inherited allocator.** `memory.json` records the stock/app baseline peaks
   as 2,291,099 / 2,294,008 bytes. The 1 MiB string fixture raises both peaks
   by exactly 1,052,357 bytes, including 3,781 bytes of surrounding evaluation
   work: equal increments, within the stated 4 KiB tolerance of 1 MiB.
   The one-byte worker ceiling refuses both as `over_ceiling`, with no admitted
   input. Its sentence is the same template and carries each process's actual
   live count. `AllocationCeiling` in the draft was not the wire category;
   the record now names the existing one. `no-count.json` proves identical
   disabled-accounting output. Feature-variant hashes are in verification.json;
   those builds preceded only moving a helper above the test module for clippy.
5. **Notebook.** The existing kernel installer installs the app worker into
   the fixture's temporary Jupyter data directory. jupyter_client executes
   answer, restarts the kernel, executes answer again and proves the old
   binding is gone. nbclient executes answer, an awaited native future and
   retained state; `extensions.ipynb` validates with nbformat. The kernel's
   ready/version check and source are unchanged. Pinned Python package
   versions are in verification.json, using the existing notebook environment.
6. **Public boundary.** cargo doc lists only the Rune re-export, Extensions
   and main_with. `api.json` preserves a successful public-import compilation
   and failed imports of private context/runner/session/worker/config/memory/
   host/extension internals, VERSION and the private builders field.
   `mismatch/` fails with E0631 when its Rune 0.13.4 Module meets rnx's 0.14.2
   Module. The earlier exact 0.14.1 pin fails Cargo resolution instead; both
   outcomes are recorded and the plan no longer promises a type mismatch for
   every mismatched pin.
7. **Regression.** Final root suites: **360 default, 401 test-support**, zero
   failures. Kernel suites: **23 each**, default and transport-probe. Formatting
   passes, release selfcheck passes, notices check passes with unchanged files.
   Clippy matches the pre-change diagnostic multiset exactly: 35 code-bearing
   occurrences, including the inherited denied non-octal Unix permission lints
   in tests/config.rs and tests/fs.rs. It is not a clean clippy exit.

## Namespace probe

The probe is guidance for trusted adapters, not a confinement test:

- Plain `function(["nested", "answer"], ...)` fails Rust compilation because
  a function name is one component; the source and compiler output are saved.
- `function("nested::answer", ...)` does not make a nested path callable from
  Rune. Both tested Rune spellings are refused.
- A native type with `item = ::other`, installed into a fixture module,
  remains `::other::Elsewhere`. That absolute type path resolves even without
  declaring an `other` crate. Declaring that crate also makes
  `other::Elsewhere` resolve. `fixture::Elsewhere` does not resolve.
- Replacing the lent sibling module with `Module::with_crate("fs")` lets
  `fs::extension_probe()` return 17. Declared-name checks cannot detect it.

The README describes these consequences and the adapter's namespace duty.
No alternative registration API was introduced.

## Matched measurements

Core 4, hyperfine `-N`, 10 warmups, 100 runs per command; no concurrent builds.
Same transitive package versions for stock and app, checked as sets of package
name/version/source and recorded with lockfile hashes in verification.json.

| Command | Before mean | Stock after mean | App mean |
|---|---:|---:|---:|
| version | 0.563 ms | 0.547 ms | 0.559 ms |
| eval 42 | 4.091 ms | 3.996 ms | 4.035 ms |
| bare run | 3.740 ms | 3.668 ms | 3.683 ms |
| 10k JSON | 11.642 ms | 11.810 ms | 11.595 ms |

Startup deltas are under 0.1 ms, within this machine's previously observed
run-to-run drift. The stock-after JSON series has a 1.187 ms standard deviation
and a 18.87 ms maximum, so its +0.168 ms mean is not evidence of a regression
or a speedup. The raw samples are retained. No speedup is claimed.

Binary bytes: before 15,111,848; stock after 15,134,384 (+22,536);
fixture app 15,205,664 (+71,280 over stock after). Hashes in conditions.json:

- before: `f1486f2bee4342159666f6b1591e66ed1285fd66f6a6d13d1531b1ce9090caa6`
- stock after: `d7f34a81c05f7e3051bc692a842d48675493af8b724dbdf54167eae3535cb37e`
- app: `536b0cd93ad8c1866b64736fe059694719bff73baa80fbf06393d307f8ddc5e3`

## Platform and scope limits

The full Windows library/binary check reaches ring and stops because the
MSVC librarian `lib.exe` is unavailable, as in earlier records. The isolated
Windows probe includes the actual extensions.rs with only the private
HostFunction data type stubbed, and passes. This is not a successful whole
library/binary Windows check, and Windows execution is unverified. The record
states this qualification rather than marking that original gate passed.

The fixture is small; real adapters can add startup cost, blocking calls and
allocations. This record proves assembly and inheritance of existing behavior,
not PostgreSQL semantics or server concurrency. Existing async CPU-loop
interruption limits remain unchanged. Raw terminal captures and tool logs keep
their original whitespace; they are evidence rather than formatted source.
