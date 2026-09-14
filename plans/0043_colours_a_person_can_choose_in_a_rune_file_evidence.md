# rnx 0043 evidence: saved presentation settings

Plan b5cc08c, revised after implementation to restrict config to sessions.
Current bench: 9868f1e, probes/settings and results/settings_0043_session_only.

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
5. The test-support open-attempt counter reports zero for run/eval/version/help
   and one for both bare rnx and explicit repl. Those session entry points load
   config even with piped input. Valid, malformed and looping configs leave
   one-shot stdout, stderr and status unchanged under auto/always/never. Palette
   equality tests now use sessions; run/eval retain flags and default colours.
   Source review limits config::load to empty arguments or repl. The counter never exists in ordinary builds. Diagnostics about
   config itself are escaped plain text before presentation initialization.
6. Measurements below preserve absent/configured conditions and whole outputs.
7. Both full suites pass sequentially: 324 default and 363 test-support,
   TERM=xterm-256color, RNX_CONFIG=/tmp/rnx-0043-scope/no-config. No dependencies or
   notices changed. Truecolour never/always captures give equal screens/cursors.

## Cost after the session-only scope correction

Bench 9868f1e, results/settings_0043_session_only. The original broader-scope
measurements remain in bench 49bc8db and results/settings_0043, labelled historical.

Measured on nano, Linux x86_64, 2026-09-14, Rust 1.98.1, Rune 0.14.2,
hyperfine 1.20.0. CPU 4, 10 warmups, 100 runs per command. All columns include
an `env RNX_CONFIG=...` executable; do not compare these absolute times with
bare-binary tables. Means ± standard deviations. Before is ba74e00 with no
config selected; after narrows loading to bare rnx/repl, including piped sessions.

| Command | Before, absent (ms) | After, absent (ms) | After, configured (ms) |
| --- | --- | --- | --- |
| version | 1.577 ± 0.028 | 1.573 ± 0.018 | 1.564 ± 0.019 |
| help | 1.570 ± 0.015 | 1.567 ± 0.023 | 1.560 ± 0.021 |
| eval | 5.088 ± 0.058 | 5.021 ± 0.038 | 5.014 ± 0.022 |
| run | 4.756 ± 0.038 | 4.679 ± 0.059 | 4.745 ± 0.349 |
| json | 12.557 ± 0.118 | 12.549 ± 0.088 | 12.584 ± 0.142 |
| session | 4.802 ± 0.099 | 4.724 ± 0.027 | 7.487 ± 0.042 |

No config-related one-shot overhead detected. Outliers were reported, including
the configured run samples; these are observations, not proof of zero CPU cost.
The structural evidence is the zero open-attempt counter for run/eval/version/help.
The configured session mean is about 2.8 ms above absent and meets the preselected
local gate of absent + 10 ms. This is not a universal latency guarantee.

Binary bytes: 14980152 → 14980216.
Session startup allocation reference, before: 1823272 bytes.
Session startup allocation reference, after: 1823288 bytes.
Session startup allocation reference, configured: 1823662 bytes.
These are allocator-request observations, not resident memory.
