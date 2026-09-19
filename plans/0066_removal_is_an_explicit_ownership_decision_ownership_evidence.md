# 0066 gate 1: ownership and filesystem prototype

Status: **ready for review**. The mount prerequisite is now exercised
unprivileged, and the ownership/filesystem candidate passes. No production
removal primitive or command has been added. Gates 2–4 remain open.

The accepted stop remains at rnx `1ab9732` and bench `1f0b85d`, both pushed.
This continuation uses root `1ab9732` as its source baseline. Candidate source,
drivers and results are in rnx-bench `94539e1`, under
`probes/removal-ownership` and `results/removal-ownership-0066`. The probe README
contains the exact rerun order, prerequisites and clean-directory requirements.
`source.json` pins every copied tool source, the candidate and its binary;
`old-tool-source.json` checks the genuine SHA-256 product against `7cd3205`.

## The mount stop is closed by an actual boundary test

Bubblewrap maps the caller's UID (1003) and prepares mounts before exec. No sudo,
sysctl change or privileged helper invocation was requested by this fixture.
The bootstrap puts an outside fixture directory under an entry using a bind
mount, and puts a tmpfs under a second child. The entry, bind and outside
directory share a device; the tmpfs does not. Mountinfo records both. The unsafe
positive control unlinks the disposable outside sentinel through the bind, so
the tested shape is a real traversal escape, not a synthetic device result.

The candidate opens below retained directory handles using
`openat2(RESOLVE_BENEATH | RESOLVE_NO_SYMLINKS | RESOLVE_NO_XDEV)`. Its regular-file
metadata opens use O_PATH with the same guard, so file bind mounts are covered
as well. It never falls back to path traversal. ENOSYS and EINVAL injection each
refuse before mutation; the normal calls run on the actual host kernel.

| Real mount shape | Device relation | Actual removal result |
|---|---|---|
| Directory bind inside entry | Same | EXDEV before rename |
| tmpfs inside entry | Different | EXDEV before rename |
| Bind at selected entry boundary | Same | EXDEV before rename |
| Bind at entries boundary | Same | EXDEV before rename |
| Bind at locks boundary | Same | EXDEV before rename |
| Regular-file bind inside entry | Same | EXDEV before rename |

All six retain their visible entry and outside sentinels. Mounts exist only in
the child namespace. The original unshare failure is still historical evidence;
it was not edited into a passing result.

## Ownership, visibility and interruption

The final matrix passes **44 cases**. These use synthetic control documents and
real filesystem operations; actual old/current layouts are exercised separately
below. List and dry-run succeed while the fixture holds the existing writer lock,
and preserve entry/control snapshots. Actual removal refuses busy cache and
runtime writer locks. Exec controls prove neither lock descriptor survives exec.
The lock inodes remain at their original paths after removal.

Selected runtime IDs refuse in selection formats 1 and 2, both for visible and
pending entries. Malformed selection refuses. Opposite-kind roots, an executable
or current directory inside the target, control-path symlinks, FIFOs and sockets
refuse. The initial user-spelled symlink to the root is accepted. Internal file
and directory symlinks are unlinked as leaves; outside data is unchanged. A
hardlink's outside name retains its content, inode, size, mtime and mode. A
read-only regular file is removed without chmod or truncation.

A full bounded preflight precedes `renameat2(RENAME_NOREPLACE)`. Replacing the
target while paused before rename produces an identity refusal and preserves
both directories. After rename, listing shows only removing/ID, never a
partially deleted entries/ID. Both parents are synced before child deletion.

SIGINT, SIGTERM and SIGKILL are each exercised before rename, after rename and
during deletion. Precommit entry snapshots stay identical. Postcommit state is
pending, with no visible entry. The committed event carries an exact shell-quoted
resume command even when SIGKILL prevents a final diagnostic. Checked signals
exit 130/143 and name pending state. Each resume removes only pending contents;
a separately created visible entry with the same ID survives. A second resume
refuses instead of targeting that visible entry. A root containing an apostrophe
also resumes through the printed `/bin/sh` command. The visible replacement is
a fixture-created tree, not yet an old-tool rebuild; that remains gate 2.

The process trace records one exec for inspection and no children. The candidate
does not invoke Git, Cargo, a compiler, a builder or the inspected artifact. Its
exec-control subprocess exists only behind the explicit prototype test hook.

## Real old consumers and current entries

The setup installs a genuine format-1 runtime from `7cd3205`, renames the original
fixture checkout away, then builds an old-key Polars assembly in a fresh private
cache. A second fixture adapter writes retained.txt to its real OUT_DIR at build
time and reads that absolute path on every `keep::read()` call. Nothing is copied
from a prebuilt assembly whose embedded paths might point elsewhere.

The current product authenticates/migrates that runtime to format 2 and builds
an independent current-format Polars assembly (receipt 4). Both source roots and
both assembly entries therefore exist during inspection. The old tool's original
product modules and lock match the published SHA-256 baseline, with executable
digests retained. Each Polars build took roughly 100 seconds with registry sources
already cached; these are setup observations, not performance claims.

An old direct session and a kernel installed against the old-key artifact are
alive together. In two rounds, the fixture changes retained.txt, runs list and
dry-run against the cache and unselected old runtime, then observes the new value
from both consumers. The session also retains its bound value 42; the kernel can
still use Polars. The fixture acquires the writer locks nonblockingly while both
readers are alive, and inspection succeeds while those locks are held. Thus the
writer lock is demonstrably neither a reader lease nor a reason to remove a live
entry.

Only after both consumers exit and are reaped does the candidate remove the old
assembly and unselected runtime. The old retained path disappears. The selected
current runtime, selection document, both projects and kernelspec remain intact;
the kept current Polars assembly still evaluates successfully. The fixture owner
then removes its own temporary kernelspec environment, without touching a user's
Jupyter installation.

Snapshots cover runtime and project content hashes plus inode/mode/size/mtime,
selection and kernelspec bytes, and cache inode/mode/size/mtime. They exclude
atime and do not hash every byte of the retained cache. The old assembly traversal
covered 3,771 descendant nodes, 1,560,141,427 logical regular-file bytes and an
allocated estimate of 1,571,573,760 bytes. These are preliminary descendant-only
counts (excluding the entry directory itself), not measured free-space recovery.
Full kept-cache byte assertions, final accounting and cost reporting remain in
the later gates.

## Checks, corrections and limits

The isolated tool is formatted and strict clippy passes across all targets in
both configurations. Its unchanged product suites pass serially: **46 passed,
2 ignored** normally; **47 passed, 2 ignored** with test support, including the
existing same-process inventory subprocess test. Original tool sources and
Cargo.lock remain byte-identical; no new dependency or root source change.
No fixture processes or mounts remain.

Three early corrections are recorded rather than attributed to the product:
duplicate/unused candidate imports found by clippy; O_PATH combined with
O_NONBLOCK, which openat2 rejects with EINVAL; and an overlong Unix socket fixture
path. The flag fix only omits O_NONBLOCK for O_PATH, leaving all resolve guards
in place. The socket fixture binds a relative name. The final matrix and real
journey pass after those corrections.

This standalone candidate requires an explicit root and emits minimal JSON.
It does not claim the finished CLI, reference annotations, complete listing
metadata/output limits or missing-root semantics. Gate 2 owns those plus the
per-parent-sync failure injections and genuine old-tool rebuild before resume.
Gate 3 owns the combined PostgreSQL journey, Polars-sized interrupted removal,
annotations and expanded retained-byte checks. No deletion primitive may be
ported into product code until this gate is accepted. The trusted-store and
quiescence boundary remains explicit; this is not a sandbox against a same-user
adversary moving already-open directories.
