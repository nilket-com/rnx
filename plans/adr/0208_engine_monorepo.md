# ADR 0208: Preserve Rune history in rnx

Status: Accepted

Import the published Rune repository under `rune/` in rnx, preserving
both original histories and signed objects. This record covers only the pure
history import. ADR 0211 owns workspace/path-dependency rewiring and compatibility
validation; ADR 0209 owns the later root commit checker. Numbers identify
decisions, not execution order.

The implementation is one signed merge commit whose first parent is this rnx
plan and whose second parent is the verified published Rune tip. Its engine
subtree equals that Rune tree exactly, including modes, licenses, copyright
and component notices. All preexisting rnx files outside the prefix stay
unchanged. Do not rewrite or squash original commits or signatures. Before the
import boundary, historical Rune paths remain root-relative.

Verify anchored bundles and restoration into empty repositories, both parent
ancestries, source and merge signatures, exact subtree and outside-prefix
equality, and a tamper rejection. Bring the isolated proof to review before
canonical integration. Preserve the original failed setup receipt as evidence.

The pure import is authorized locally. Publication belongs to the reviewed
canonical migration sequence after ADR 0211 compatibility checks; do not archive
the source repository merely because the import exists. Archive Rune read-only
only after verified canonical publication and reviewed redirect material. Keep
old tags and hashes available to historical git consumers. Deletion requires
a distinct later explicit user instruction. No force-push or history rewrite
is authorized.
