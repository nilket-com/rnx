# rnx 0029: the notices a binary must carry

Status: proposed 2026-09-10. The twenty-ninth record of rnx. Record 0028
established that a distribution needs a third-party notice file and stopped
there, because its own scope was an inventory. This record builds the file,
from the packages that are actually resolved and the licence texts those
packages actually ship.

It settles no licence question. Record 0001 decision 7 still owns rnx's own
licence, and whether these notices satisfy any particular distribution is a
review against that distribution, not a property of a generator.

## Context

Measured, and two of the three surprised the record that asked for this:

- The resolved set is **53 packages** — normal dependencies with the
  procedural-macro subtrees pruned, queried per target and unioned across
  Linux and Windows.
- They ship **31 distinct licence texts** between them. The Apache-2.0 text is
  byte-identical wherever it appears; the MIT texts are not, because each
  carries its own copyright line, which is the part MIT asks to be reproduced.
- **Eight of the 53 ship no licence text at all.** They declare one in their
  metadata and include no copy of it: `rune`, `rune-alloc`, `rune-core`,
  `rune-tracing`, `musli`, `musli-core`, `syntree` and `clipboard-win`. The
  Rune family is most of that list, which makes it rnx's own problem rather
  than a detail about a transitive dependency.
- **Seven of those eight are recoverable, and one is not.** Every published
  package records the revision it came from in `.cargo_vcs_info.json`. At
  those revisions the texts exist for all but `syntree`, whose repository
  contains no licence file at all — checked against the tree at
  `cf71c9e`, which holds `Cargo.toml`, `README.md`, sources and nothing else.

The third was invisible to record 0028 and to every SPDX-based summary,
because a declaration in metadata is not a text in a package, and only opening
the directories tells the two apart.

## Decision

### 1. Generated from the resolved set, never written by hand

`scripts/third-party-notices.sh` asks Cargo for the set — `cargo metadata` for
the packages and their directories, `cargo tree -e normal,no-proc-macro` per
target for the scope — reads every file whose name looks like a licence,
notice, copying or copyright, and writes `THIRD-PARTY-NOTICES.md`.

Deterministic: sorted throughout, no dates, no paths from outside the
repository, and texts deduplicated by content so a text shared by thirty
packages is written once and attributed to all of them. Two runs produce
identical files, which is what makes the check below possible.

### 2. Source and binary coverage are described separately, and neither is declared settled

The file says both, and an earlier draft said the first of them too strongly:

- **A binary links these packages.** Their licences ask for their notices to
  accompany it, this file is what accompanies it, and a release that ships a
  binary without it is incomplete.
- **A source distribution redistributes none of these packages** — Cargo
  fetches each dependency at build time, from its own registry, under its own
  terms.

What the second does **not** establish is that a source distribution carries
no third-party material. Record 0028 records three files in rnx written by
reading upstream Rune — the JSON pre-walk, the budget sentinel, the escape
decoder — and leaves open whether a mirrored interface warrants attribution.
The first draft wrote "a source distribution contains no third-party code",
which walks straight past that open question. The file now describes what the
source package redistributes and names the derivation question as unresolved,
which is the most that is known.

### 3. The set is the scope that was selected, which is not an upper bound

Record 0028's guardrail carries over: a dependency-graph query identifies
**candidates for distribution review** and does not establish what an artifact
contains. The file repeats it rather than relying on 0028 being read.

An earlier draft went further and called the set an upper bound on the
material in a binary. It is not one, and the counter-example is in the
exclusion itself: a derive macro **emits code that is then compiled in**, so
pruning `serde_derive` and its kind excludes *packages that are linked*, not
*material that appears*. Build-time packages are excluded on the same basis
and carry the same caveat. `cfg_aliases` is named anyway — build-only, reached
through `nix` on unix, shipping `NOTICES.md` with MIT terms for code from
`tectonic_cfg_support`.

So the file describes the scope it selected and the questions that scope
leaves open, and makes no claim about what an artifact contains in either
direction.

### 4. A text the package omits is fetched from the revision it was published from

The first draft of this record refused this and was wrong to. It reasoned that
fetching from a project's repository would attribute "a text that was not the
one distributed" — but `.cargo_vcs_info.json` records the **exact revision**
each package was published from, so the file at that revision is not a guess
about the project's licensing; it is what stood in the project when that
package was cut. The package omitted it from the tarball, which is a packaging
fact, not a licensing one.

`scripts/fetch-missing-licenses.sh` does it: read the revision and the
repository, try the usual filenames at that revision, and check in what it
finds under `third-party/licenses/<package>-<version>/`, with the revision,
path, URL and SHA-256 in `SOURCES.tsv`. **Seven of the eight are answered**
that way — thirteen files, covering both halves of each dual licence.

The distinction is kept in the file rather than blurred: those texts appear
under a heading that says they are not from the package, with the revision and
path beside each. A bundled text and a fetched one are different evidence and
are presented as different evidence.

The network is needed **only** to refresh them. The generator reads the
checked-in copies and verifies each against its recorded digest, so an
ordinary build and an ordinary test run touch nothing outside the repository.

What is still refused: substituting a canonical MIT text, which would mean
**inventing a copyright holder**, since that text has a blank where the name
goes. `syntree` has no text at its revision and none in its repository, so it
stays in the unresolved section — one package rather than eight, and an
upstream conversation rather than a guess.

### 5. Every step that can fail, fails — and writes nothing when it does

The first version of the generator suppressed Cargo's errors and ignored its
exit status. Review substituted a `cargo tree` that exits 42: generation
reported **success** and wrote a file describing zero packages, and `--check`
then agreed with it. A notices file that reports nothing is not an empty
result; it is the silent removal of every attribution rnx carries.

So: `set -euo pipefail`; each Cargo invocation's status checked and its stderr
quoted in the failure; a package whose manifest cannot be found stops the run
rather than being skipped; digests and copies checked; and floors on the
resolved set and the text count, because "succeeded and found nothing" is the
shape the defect took. Nothing is written until the whole run succeeds, so a
failed run leaves the existing notices in place.

**A floor on the union is not a floor on a target.** The first repair checked
the accumulated set, which is non-empty as soon as one target has contributed
— so a second target answering nothing at all passed unnoticed, and review
produced exactly that: 53 packages became 49, Windows coverage gone, exit 0.
Each target's own filtered output is now checked before it is appended. The
filtering moved from a pipeline to one `awk`, because `grep -v` answers 1 for
"nothing matched", which is indistinguishable from an error and was being
swallowed by a `|| true`.

**Publishing is a switch, not a deletion followed by a copy.** The fetcher
removed the destination and copied the replacement into it, so a failure
anywhere in that copy left nothing: review injected one and watched fourteen
files become zero. The replacement is now assembled **beside** the
destination, the previous set is moved aside rather than removed, and it is
put back if the switch fails. A fetcher that deletes the licences it exists to
keep is worse than one that never ran.

**A recovery copy is never the next run's to delete.** The switch leaves the
previous set in `licenses.previous` when a rollback fails, which is the whole
point of keeping it — and the next run's startup then removed it
unconditionally, so a second failure left nothing at all while still reporting
that the previous licences were unchanged. A salvage is now put back before
anything else happens, or refused outright if the destination exists too,
because then only a person can say which of the two is right.

**And a request that fails is not a file that is absent.** The fetcher treated
every non-200 alike, so a server answering 503 reported "no licence found" for
every package and left `SOURCES.tsv` holding its header — an outage converted
into the claim that nothing exists anywhere. Only 404 now means a candidate
path is not there; anything else stops the run. Texts and provenance are
staged and published together, so a run that fails part-way leaves the
previous set exactly as it was.

`CARGO` selects the Cargo to use, which is how the gate below substitutes one
that fails.

### 6. Four gates, because a notices file rots quietly

- `--check` regenerates and compares, so a dependency that arrives or moves
  fails the suite rather than leaving the file describing last month's set.
- The file, `SOURCES.tsv` and the generator must all appear in `cargo package
  --list`: notices that do not ship cannot reach whoever builds a binary.
- A failing dependency query must write nothing and say so — asked twice, for
  a query that fails on every target and one that fails on a single target,
  because the second leaves a plausible-looking set that is short by one
  platform.
- Every fetched text must match the digest recorded when it was fetched.
  Those copies did not come from the packages that declare them, so the
  digest is the only thing tying one to its origin.
- A request that fails must not be recorded as a licence that does not exist,
  and must leave the previous record alone. Driven by a stub `curl` answering
  503, which also keeps the gate off the network.

All are Unix-only: the generator is a shell script, the same boundary the
packaging gate draws. **And each asks its question of the copy it is in** —
record 0026's distinction, because Cargo refuses to package an extracted
package, and the first draft of the shipping gate ran `cargo package --list`
unconditionally and failed inside the very artifact it was checking.

The correction to that first draft then overshot the other way: it accepted
any file with two substrings in it, and let the regeneration gate return
without checking anything at all, so the packaged notices were verified by
nothing. Both are now complete checks. The regeneration gate runs in **both**
copies — the generator and its inputs ship, and Cargo resolves the same locked
set inside the package, so the whole file is rebuilt and compared there too.
The shipping gate, in the repository, unpacks the package and compares the
notices **byte for byte** with the ones the repository validated.

A gutted file fails the regeneration gate in the repository and inside the
package; both were measured.

## Acceptance gates

All met, on Linux, at this commit.

1. **Generated, not written.** `scripts/third-party-notices.sh` produces the
   file; two consecutive runs are byte-identical.
2. **The set matches the resolved graph.** 53 packages, 34 distinct texts, 13
   fetched from published revisions, 1 with no text anywhere.
3. **Staleness fails the suite.** Controlled: deleting one package's row makes
   `--check` fail with "out of date", and the gate reports it.
4. **The notices ship, with their sources.** Controlled: adding the file to
   `exclude` fails the gate, which then names what the package does contain.
5. **A failed query writes nothing.** Controlled: with the suppressing shape
   restored, the gate fails with "a failed query was reported as success".
   Asked for a complete failure and a single-target one.
6. **A target that answers nothing stops the run.** Controlled: a Cargo that
   exits 0 with no output for the Windows target now fails with "cargo tree
   returned no packages for x86_64-pc-windows-msvc", and the notices are
   untouched — where it previously dropped four packages and reported
   success.
7. **A fetched text matches its record.** Controlled: appending a line to one
   of them fails the gate by name.
8. **A failed request is not an absence.** Controlled: a stub `curl` answering
   503 makes the fetcher exit non-zero naming the response, and `SOURCES.tsv`
   is unchanged. With the old every-non-200-alike shape restored, the gate
   fails with "a server that answered 503 was taken for a missing file".
9. **A gutted file is caught in both copies.** Controlled: replacing the
   notices with a heading and one row fails the regeneration gate in the
   repository and inside the extracted package.
10. **A failed publication keeps the previous licences.** Controlled: with the
    delete-then-copy shape restored, the gate fails with "a publication that
    failed did not leave the previous licences alone". The fetch half
    succeeds first, through a stub `curl` answering 200, so the failure is
    the switch and not the search.
11. **A retry does not delete what the first attempt salvaged.** Both attempts
    run: one fails its publication *and* its rollback, leaving the licences in
    the recovery directory; the second must put them back rather than remove
    them. Controlled: with the unconditional startup removal restored, the
    gate fails with "after the retry there are no licences at all", quoting
    the script's own claim that they were unchanged.
12. **The comparison itself cannot fail quietly.** The helper that lists a
    directory with digests walks it in Rust, passes the paths to one
    `sha256sum` as arguments rather than through a shell, checks the status
    and the line count, and treats an empty directory as a failure. Its first
    version ran a `find | xargs` pipeline without `pipefail`, ignored the exit
    status, and interpolated an unquoted path — so a broken `sha256sum` made
    two listings equally empty and a control passed over deleted files.
    Controlled: with `sha256sum` replaced by one that exits 3, the gate fails
    naming the directory.
13. **The packaged copy runs its own ladder.** Extracted and run, rather than
    inferred from a file list.
8. **Nothing regresses.** The ladder passes **242 tests and 247 with
   `test-support`**, zero failures; clippy stays at its eleven pre-existing
   warnings; `cargo fmt --check` is clean; and the Windows and both macOS
   cross-target checks report nothing in either feature set. The extracted
   package runs **242** of its own.

   One flake was seen and is recorded rather than hidden:
   `a_failure_after_cleanup_begins_still_reaches_the_script` failed once under
   load, reporting exit 0 where it wants 1 — the injected delivery failure not
   firing before the call ended. Five targeted reruns and four full suites
   since have all passed. It belongs to record 0022's machinery, not to this
   cut, and it is the shape worth watching: a gate that passes when its
   injection never happens is a gate that can agree with silence.

## Guardrails and stop conditions

1. This record chooses no licence and declares no obligation satisfied.
2. The generated file is never edited by hand. A correction is a change to the
   generator.
3. A dependency-graph class identifies candidates for review; it never
   establishes what an artifact contains.
4. A missing licence text is reported, never substituted or invented. A text
   fetched from a published revision is labelled as such, never presented as
   one the package shipped.
5. **A destructive control runs against a scratch copy of the repository, not
   the repository.** These controls break the generator and the fetcher on
   purpose, and a broken script destroys things — that is what is being
   tested. Run against the repository they damage tracked files and, worse,
   they do it while other tests read the same directory: review reproduced
   `every_fetched_licence_matches_its_recorded_digest` failing because a
   control had briefly deleted its input. A copy removes the race and the
   need to repair anything afterwards.
6. **A control cleans up before it asserts.** An assertion before the restore
   leaves the wreckage behind. That is the third time this session that
   ordering has mattered — the harness controls in `tests/child_input.rs` were
   the first two — so it is written here as a rule rather than remembered as
   an incident. It also restores **whole**: this gate's first
   version put `SOURCES.tsv` back and left the thirteen texts beside it
   deleted, and reported success.
5. Whether the notices are complete for a given distribution is a review of
   that distribution. This record produces the input to it.

## Risks

- **The eight texts are a real gap, not a formality.** For the MIT half of
  those dual licences there is no copyright notice in the package to carry,
  and rnx's own dependency is four of them. If the licence review needs them,
  the answer is upstream — an issue asking the crates to include their texts —
  and that is a decision, not a build step.
- **Two targets, not every target.** The union covers Linux and Windows, which
  is what rnx builds for today. A third platform adds packages, and the
  generator must be rerun for it rather than assumed to cover it.
- **`--check` needs Cargo and the registry.** It is a test that shells out and
  reads the package cache, so a machine with neither cannot run it. It is
  Unix-only for the same reason the packaging gate is.

## Forward

The licence itself, which record 0001 decision 7 owns, and with it the
question of whether these notices are complete for the way rnx is actually
shipped. The `cfg_aliases` question. And, if the review wants the eight texts,
an upstream conversation with the projects that do not ship them.
