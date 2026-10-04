# rnx 0167: source formatting with tabs — evidence

**Status:** implementation ready for review; nothing from this record pushed.
Plan 6e646eb. Final measurements ran on clean code a787006; the evidence amend
changes documentation only. Benchmark artifacts are committed as rnx-bench 5b40205,
`probes/fmt-0167` and `results/fmt-0167`. Application source remains external.

## 1. Result and boundary

`rnx fmt FILE...`, `--check FILE...`, `--stdin`, `--help` and `--` work in the
stock and feature-off commands. Structural indentation is a literal TAB;
adjacent function items have a blank line, including module/impl functions.
Attributes and leading comments stay attached. Multiline literal/template and
comment payload bytes remain protected. No source executes and no adapter,
project build, network, runtime or Rune Context is created to format it.

The implementation uses pinned Rune 0.14.2's public formatter with recovery
false, not a fork. Its public tokens and AST identify spans. A strict parse
precedes formatting; another parse and exact token-class/lexeme and comment
checks follow it. Rune token kinds carry position metadata, so fidelity compares
the enum discriminant plus exact source bytes, not position-bearing equality.
Synthetic overlapping template tokens are collapsed to their original whole
literal span. Only newly inserted comma tokens are removed after ordered
alignment proves every original token remains; an original comma cannot vanish.
The complete pipeline runs twice and must produce identical bytes.

This is a formatter, not a linter or full rustfmt style compatibility promise.
Valid source that 0.14.2 cannot format faithfully is refused without publication.
In particular the known lone-brace/escaped-backtick template defects are named
fidelity refusals. A future Rune upgrade should replay all controls before
removing any workaround.

## 2. Worker and publication

The private worker dispatches before ordinary startup/context setup. A control
reports one worker thread and zero Context constructions; the audited path only
constructs parser/Sources/formatter objects. The parent uses three bounded I/O
threads, not the worker. Framed input/output carry magic, exact payload length
and a request fingerprint. The fingerprint binds the same-binary request; it
is not a cryptographic authenticity claim. Malformed, truncated, oversized,
wrong-length or wrong-request packets fail closed.

Linux-only limits: 128 explicit files; 1 MiB input each, 16 MiB total; 4 MiB
output each, 32 MiB total. Metadata and bounded reads check remaining input
before allocation, and the parent caps each output read by the remaining total.
Worker limits before parsing are 512 MiB address space, 8 MiB stack, 2 seconds
CPU and a parent 5-second wall deadline. Workers run sequentially. Panic,
protocol failure, output overflow and timeout are refused; the timeout control
is killed and reaped. Ordinary builds cannot enable controls via environment.
Other platforms return a named refusal until equivalent bounds exist.

All inputs are regular, UTF-8, nonblocking-opened/fstat-checked on the same
handle. Symlinks are refused in both modes; hardlinks in write mode. Duplicate
file identities are refused before formatting. Every file is formatted and
validated before staging starts. Create-new sibling stages preserve permissions
and are synced; each target's identity/content is rechecked before rename.
Unchanged files retain their inode. Stages are cleaned on every tested failure.
Each rename is atomic; the set is not a transaction. The injected second-rename
failure reports the first replacement and completed count, leaving the second
unchanged. An injected edit is preserved and refused. This observes concurrent
edits, not a filesystem compare-and-swap guarantee.

## 3. Capacity, costs and regression gate

Final resources are in `framed-final/`; earlier `final/` and root-level probes
are preliminary and retained separately. The selected cap is 1 MiB rather than
the provisional 2 MiB. The full fidelity/idempotence pipeline processes a dense
1,048,575-byte valid fixture (one byte below the cap):

| Measure | Final observation | Bound |
|---|---:|---:|
| CLI wall time | 1.296 s | worker 5 s |
| CLI + reaped-worker CPU | 1.275 s | worker 2 s |
| Worker virtual peak | 350,928 KiB (343 MiB) | 524,288 KiB |
| Worker RSS high-water mark | 260,580 KiB (254 MiB) | no separate RSS bound |
| Worker threads | 1 | lean control |

CPU includes parent plus worker and is an upper bound on worker CPU. RSS/VSZ
are sampled from `/proc`; the retained high-water/virtual-peak fields describe
observed process peaks. This is one measured realistic workload, not a proof
all valid 1 MiB sources fit. Excess work is a bounded refusal. The typical
0166-shaped source's end-to-end `fmt --check` took 21.85 ms. There is no new
numeric formatter-latency gate.

The raw unfinished-macro control deliberately bypasses strict preparse. It
reached 494,956 KiB virtual peak, then returned `Failed to format source`
(exit 2) under the AS bound in 0.419 s. It was an allocation refusal, not a
signal kill. The separate sleeping-worker control reached the wall deadline
and was killed/reaped in 5.03 s.

Stock binary: 20,360,568 bytes versus baseline 20,100,888 (+259,680, +1.29%).
No Cargo.lock or dependency-set change; Rune's existing `fmt` feature is enabled.
Six warm interleaved rounds retain 7,200 samples on one allowed CPU. The
pre-stated 5% aggregate-median startup/cell gate passed:

| Clock (ms) | Before | After |
|---|---:|---:|
| version | 1.8793 | 1.8807 |
| eval | 5.4146 | 5.4068 |
| JSON run | 13.2344 | 12.9424 |
| first prompt | 4.1966 | 4.1818 |
| persistent cell | 0.35136 | 0.35151 |

Round medians, p10/p90, raw samples, commands and binary hashes are in
`startup-final/`. The baseline was retained from 89c9ce5; a recorded zero-byte
diff of relevant source against closed ffa2d4e establishes code equivalence.
The earlier preliminary timing run briefly overlapped a baseline build; it is
not the deciding run. Final timing ran without Cargo builds by this team.
These clocks include process/PTY overhead and describe this machine only.

## 4. Functional checks and external application

Five core formatter tests cover tabs/gaps, nested items/comments/templates,
literal bytes, comma/token mutation refusal and framing. Eight CLI integration
tests cover file/check/stdin statuses and stdout purity, late syntax failure,
UTF-8 and caps, aliases/permissions, FIFO/directory, dash-prefixed names,
unchanged inode, worker controls, staged cleanup, partial publication and edit
refusal. Six additional reported upstream shapes are exercised within that
suite: lone template `{`, lone `}`, escaped backtick, both token-joining
examples from f0f6884f, and `# !`. All are refused, stdout stays empty and the
original files remain byte-identical. The first three fail fidelity; the latter
three fail strict parsing before calling the upstream formatter.

All six committed demos format and pass repeat `--check` without adapter
loads. The three offline demos' outputs are unchanged when executed before and
after formatting. Adapter demo execution is not credited here; fidelity and
idempotence are checked without their external dependencies.

The external `site.rn` has been formatted locally by this pipeline, showing
tabs and blank function gaps. Its separate formatting-only diff and local
before/after wire checks are retained in rnx-bench: 26/26 exact HTTP fixtures
and three complete responses over one keep-alive connection pass both times.
The final binary's `fmt --check` also exits 0. No application commit, push or
deployment is performed by this record; the application diff is reviewed
separately. Application-specific source/content is absent from rnx's commit.

## 5. Suites, attempts and upstream status

Retained logs and count summaries are in `checks/`. The final root test-support
suite, counted feature-off suite, project suite, formatter tests, fmt, clippy
and diff checks pass. Root: 523 passed, 3 ignored. Project: 105 passed,
2 ignored; counted feature-off: 429 passed. Clippy retains only inherited
warnings outside the new formatter; a feature-off unused test-only import was
noticed and gated before the final builds.

Failed attempts are not erased:
- Provisional 2 MiB and early 1 MiB fixtures hit the 2-second CPU limit while
  edits rebuilt Strings repeatedly. A linear forward assembly replaced that
  quadratic path; the chosen 1 MiB capacity now passes with measured headroom.
- Template synthetic spans and a trailing-comment range initially caused
  refusal/panic during development; protected whole-template spans and bounded
  comment attachment corrected them. Permanent fixtures cover those shapes.
- Full token-kind equality initially compared position payloads and rejected
  harmless formatting. Discriminant plus exact lexeme comparison fixes that;
  mutation controls prove changed bytes/classes still fail.
- Uncounted feature-off testing hits two allocation-ceiling tests that also
  fail at baseline ffa2d4e. They are not formatter regressions. The counted
  feature-off configuration is the passing gate; baseline failures are retained.
- Running two Cargo feature configurations concurrently replaced the shared
  `target/release/rnx` while an async budget test was invoking it, causing a
  missing test-support function. Serialized configuration runs pass. This was
  a test invocation collision, not a product flake fix.

Three unfiled upstream drafts accompany the implementation. Claude checked
main `bb8e69372353c50e271c9f115bc771c77aa6b83e`; Codex replayed the same scratch
program under OS bounds and retained it in `upstream/`:
- The unfinished-macro allocation defect is fixed on main; the exact fixing
  commit was not bisected. That draft asks for release of the existing fix.
- Silent completion of `foo(1,` still happens on main with recovery false;
  that remains a live defect report.
- `fmt.indent=tab` already exists from 20b26957; that draft asks for release,
  not a new feature. Template corruption is also fixed on main (e9ae73fb).

Nothing is filed automatically. This record stays on the released 0.14.2 pin;
upgrading to the unreleased compiler/grammar is separate work.
