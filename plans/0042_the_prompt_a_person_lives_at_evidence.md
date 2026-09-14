# rnx 0042 evidence: the compact prompt

Plan 8978a15. Bench e272789, probes/prompt and results/prompt_0042.

Both suites pass, 313 default and 351 test-support, sequentially under
TERM=xterm-256color. New pipe tests gate alias/help, clear silence, flag order,
splash and argument placement. Completion includes :q and :quit in the same
snapshot as all other command completions (source review). Existing tests changed
only at the named prompt fixtures and the inspector's command-count fixtures
for expanded help. The HTTP reader now matches a numbered prompt, because a
bare `> ` suffix could mistake a help signature ending in `-> ` for a prompt.

Regular PTY tests gate clear under color=never, unchanged number and bindings,
recorded command history, title push/set and exactly one pop on :q/:quit/EOF,
no pop on prompt Ctrl-C, and explicit pop on file completion, panic, host::exit.
Title text goes through terminal_safe, including filenames, before it is placed
inside OSC. Return errors take the scope guard; every direct process exit in
main and host goes through terminal::exit. This is protocol evidence; title
stack support and restoration of a terminal's previous title remain best-effort.
The screen emulator proves clear places the unchanged prompt on row zero and
never/always produce identical final screens and cursors. Windows is unexecuted.

Measured on nano, Linux x86_64, 2026-09-14, Rust 1.98.1, Rune 0.14.2.
Hyperfine 1.20.0, CPU 4, 10 warmups, 100 samples. Means ± sample standard
deviation; raw exports, binary hashes and allocation observations are alongside.

| Command | Before ms | After ms |
| --- | --- | --- |
| version | 0.556 ± 0.017 | 0.540 ± 0.016 |
| help | 0.548 ± 0.015 | 0.537 ± 0.014 |
| eval | 4.061 ± 0.012 | 4.024 ± 0.056 |
| run | 3.733 ± 0.065 | 3.679 ± 0.019 |
| json | 11.625 ± 0.123 | 11.607 ± 0.100 |

Binary bytes: 14879080 → 14916704.
Outliers were reported; these samples are not an attributed speedup.
