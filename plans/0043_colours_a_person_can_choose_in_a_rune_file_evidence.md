# rnx 0043 evidence: saved presentation settings

Plan b5cc08c. Bench 49bc8db, probes/settings and results/settings_0043.

## Gates

1. Missing, blank and empty-object files are silent; directory and permission
   refusals warn and still start. Non-Unicode/relative explicit paths warn.
   Unix absolute XDG and relative-XDG/HOME fallback are exercised. Windows
   APPDATA/LOCALAPPDATA gates exist but have not executed. Final suite commands
   set an isolated missing RNX_CONFIG inherited by subprocesses; config-specific
   gates override it with their own fixtures. Green prompt assertions are the
   only existing test expectation changes.
2. All sixteen ANSI names and mixed-case six-digit RGB values are unit-tested,
   malformed/escape-bearing values rejected. New pipe equality gates strip only
   the chosen palette's sequences and compare complete stdout/stderr with never.
   Error bold and separate caret foreground are preserved. Regular PTY gates
   pin RGB keyword, prompt, result and error bytes and dim comment attributes.
3. Parse, runtime panic, shape, instruction budget and source-size failures each
   warn once and reach the session. Valid neighbours of bad fields are retained;
   warnings are in sorted-key order. A local helper computes a colour. fs/http,
   println/dbg, external mod and top-level await fixtures fail without producing
   config output. NoopSourceLoader is explicitly installed; there is no file
   loader fallback. A FIFO fixture returns without a writer within the harness's
   ten-second bound; regular-file reading shares 0035's open-and-handle check.
   As agreed, native-call time, filesystem stalls and process-wide exhaustion
   are not bounded by the evaluator. It is not an allocation sandbox.
4. CLI mode overrides the saved mode, saved always colours pipes, nonempty
   NO_COLOR still disables auto, and splash precedence works both ways.
5. Run and eval diagnostics both use saved error colours. The test-support
   open-attempt counter reports zero for version/help and one for session/eval.
   Source review places config loading after both fast paths and limits it to
   run/eval/repl. The counter never exists in ordinary builds. Diagnostics about
   config itself are escaped plain text before presentation initialization.
6. Measurements below preserve absent/configured conditions and whole outputs.
7. Both full suites pass sequentially: 324 default and 363 test-support,
   TERM=xterm-256color, RNX_CONFIG=/tmp/rnx-0043/no-config. No dependencies or
   notices changed. Truecolour never/always captures give equal screens/cursors.

## Cost

Measured by Codex on nano, Linux x86_64, 2026-09-14, Rust 1.98.1,
Rune 0.14.2, hyperfine 1.20.0. CPU 4, 10 warmups, 100 samples per command.
Every command goes through `env RNX_CONFIG=...` to vary config in the same run;
that executable's launch overhead is included in ALL columns. Absolute times
are therefore not comparable to earlier bare-binary startup tables. Means ±
standard deviations below; commands, hashes, config and raw samples accompany it.
Before is the accepted 0042 release, 9e17aad; after implements plan b5cc08c.

| Command | Before, absent (ms) | After, absent (ms) | After, six colours (ms) |
| --- | --- | --- | --- |
| version | 1.575 ± 0.023 | 1.559 ± 0.017 | 1.562 ± 0.020 |
| help | 1.571 ± 0.021 | 1.573 ± 0.039 | 1.566 ± 0.019 |
| eval | 5.096 ± 0.026 | 5.017 ± 0.017 | 7.743 ± 0.035 |
| run | 4.759 ± 0.027 | 4.682 ± 0.029 | 7.417 ± 0.034 |
| json | 12.714 ± 0.164 | 12.504 ± 0.104 | 15.197 ± 0.100 |
| session | 4.803 ± 0.028 | 4.729 ± 0.029 | 7.506 ± 0.128 |

The six-colour config adds about 2.8 ms to the session mean. This passes the
preselected local gate: configured session <= absent session + 10 ms. That is a
measurement tolerance on this machine, not a universal startup promise. No
regression is detected without a config in this run; small decreases are not
attributed speedups. version/help timings are independent of whether the config
file exists, and the test-support open counter establishes zero config attempts.

Binary bytes: 14,916,704 → 14,980,152 (+63,448). Session allocation startup
reference: before 1,823,272; after absent 1,823,288; configured 1,823,662 bytes.
These are allocation request observations, not resident memory. Palette strings
remain; the config context, unit, VM and returned value are dropped.
