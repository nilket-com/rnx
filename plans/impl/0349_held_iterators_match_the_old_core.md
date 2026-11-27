# Implementation plan 0349: Held iterators match the old core

Status: Proposed

Paired ADR: ../adr/0349_held_iterators_match_the_old_core.md.

1. The boundary was reported and ruled on before this plan: the
   causes, the refusal of Decision 2, the reading of Decision 3,
   the list of Decision 4 and the cost findings of Decision 6.
   Rebase the work onto the published base.
2. The refusal of Decision 2, with Controls 1, 2 and 7.
3. The calls of Decision 3 and the messages of Decision 5, with
   Controls 3, 4 and 5.
4. Plant each old behavior back. Replay the public admission
   corpus against the published base. Send the list for review
   before signing if any source changes its exit, output or
   effects.
5. Score the file gate and both project screens. If a row is
   outside its band, stop and send the counts.
6. fmt, Clippy, preflight and the targeted checks in separate
   state. Then the stages of Control 11 by their stage commands.
   Sign. Source review, one complete final, Results.

## Results

The file boundary replay used the same script path on the published dispatcher
`0f5efd6b` and the candidate. All 576 witnesses matched forced stock in exit,
stdout, diagnostic and position, in both native modes. Of the 102 witnesses
that used to fail where stock succeeded, 79 now fall back before execution
and 23 now succeed on the execution engine. All 27 cases where both engines
failed now agree on the diagnostic and its position. In the separate public
corpus, all 6,404 comparisons kept the same outcome and route. Both builds
had 67 legacy-only entries in each native mode.

The following lists all changes of route or refusal label in the boundary
replay. Braced alternatives denote every combination, in the spelling used
by the witness IDs. Counts are per native mode.

| Witness IDs | Count | Base → candidate |
|---|---:|---|
| `provided-{arg_skip,arg_take_for,closure_skip,cond_skip,ret_chars_skip,tuple_skip,vec_store_skip}` | 7 | Execution (InvalidCode) → legacy (Method) |
| `{chars,strsplit,charsplit}-{skip,take}-{return,arg,vec,tuple,closure,branch}-{next,for}` | 72 | Execution (InvalidCode) → legacy (Method) |
| `chars-skip-unused`, `chars-take-unused`, `strsplit-skip-unused`, `strsplit-take-unused`, `charsplit-skip-unused`, `charsplit-take-unused` | 6 | Execution → legacy (Method), same success |
| `valid-chars-skip-isreadable`, `valid-chars-take-isreadable`, `valid-strsplit-skip-isreadable`, `valid-strsplit-take-isreadable`, `valid-charsplit-skip-isreadable`, `valid-charsplit-take-isreadable` | 6 | Execution → legacy (Method), same success |
| `{chars,strsplit,charsplit}-{skip,take}-isreadable` | 6 | Legacy (FrontendDiagnostic) → legacy (Method), same missing-item diagnostic for the misspelled observer |

Local consumption keeps its route. The two pinned unknown-receiver `get(0)`
calls on char skip/take now refuse with Method. The installed-method policy
also refuses the eight escaping text-adapter results exercised by its
held/inline receiver fixtures. The gap-hunt contract rows needed no edits.

The installed-context check joined 684 registrations on 17 concrete owners
to their method and specialization. It exercised 300 admitted pairs and
384 compile-time refusals. The owners were the slice, object and option
iterators, chars, split by char and by string, skip, take, and the half-open,
inclusive and open range iterators for each of i64, u64 and char. The two
represented split specializations have distinct hashes. The third installed
split takes a predicate function and has no corresponding heap owner.

The six repaired pairs were length on the four finite integer range
iterators and on the object iterator, and clone on the option iterator.
Both clone spellings preserve the option cursor's position and give the copy
an independent cursor. Length on a slice iterator retains the missing-method
error. An exhausted take reports zero before querying its child. Free clone
now identifies the concrete owner in 25 diagnostic witnesses, and skip/take
length over chars reports the child's missing LEN in the other two. The
heap controls checked zero remaining entries and payload bytes after release.

Finite open-range prefixes matched stock. At i64::MAX and u64::MAX, stock
release yields the maximum then wraps, while the execution engine reports
Numerical overflow before the first yield. At char::MAX, stock panics in
Step::forward while execution reports Numerical overflow. These three
ADR 0264 differences reproduced with native off and on.

The file gate passed all 32 per-record and 32 cumulative rows. Both
matched project screens passed all 128 rows combined. Store::get cost 45
instructions per call on both images (400,000 calls per size-five vector
fixture). Store::len cost 35 on both (55,345 calls in size-five native-off
Sieve and 20,156 in native-on Permute). The no-detail store on both images
wrote one zero byte at offset zero, size 32 and alignment 8. Native-on file
Permute execution used 20,322 fewer instructions at size one and 20,394
fewer at size five. The declared invoke_args transition was the sole
changed item in the forty-item source inventory.

Restoring each of five old behaviors made its specific test fail: allowing
escape, rejecting held lengths, rejecting option clone, naming every clone
receiver as a slice iterator, and discarding the child's missing LEN detail.
The six focused positive controls passed. An initial local invocation enabled
doc without its required languageserver feature and stopped at compilation.
The positive controls used the feature combination in the native manifest.

Formatting, the four manifest Clippy configurations, preflight and the full
rune-suite, root-tests, cross and final-controls stage commands passed before
signing.

The complete final ran on be2a1bb0 as FINAL-v1 and passed all 107
stages. RESULT.json has sha256
fc4057bb2f9de5a4244c4247243c0202a352561f1e91c8877f5c85f812b211b6.
Entry through cleanup took 4,265.6 seconds against the 5,100 second
limit. The run entered with 190,820,691,968 bytes free against the
151,152,450,560 required. The longest chain was the gate, which ran from
3,673 to 4,224 seconds. The Rune suite ended at 2,286 seconds with
1,021.4 seconds in the contracts. The declaration changed the policy
file, so no published gate inputs matched and the gate rebuilt and
measured the base on the clock. The anchor lookup missed and the anchor
build took 97.1 seconds. The source condition passed over 40 items and
accepted the one declared transition, invoke_args, with no other item
changed. All 32 per-record rows passed and 17 were exact. Four native-on
rows rose, each by less than 0.1 per mille against the 5 per mille band:
awfy_sieve W, vec_iter_i64 W and T, and vec_iter_bool W. The other 11
fell. The largest falls were in native-on awfy_permute: 20,322
instructions at size 1 and 20,394 at size 5 in W, and 21,612 and 21,684
in T. The eight installed native-on T rows were scored under version 2,
each with 14 added relative relocations and 150 loader instructions. In
shared_rows the loader was the whole rise in setup. All 32 cumulative
rows passed. The fetch stayed offline and took 11.6 seconds. The build
peaked at 121,045,188,608 bytes and shared memory at 987,136 bytes. The
cleanup cleaned 37 targets and removed the run. The memory controller
reported no events. The run left no new entry in /tmp or /var/tmp.
