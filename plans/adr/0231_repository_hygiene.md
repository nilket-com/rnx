# ADR 0231: Repository-local tooling and tracked-file hygiene

Status: Accepted

Tools must run without private machine paths or agent-notification infrastructure.
They write terminal receipts and return nonzero on failure; external launchers
own notification. Remove the unused notification-protocol helper and switches.
Use repository-relative paths or explicit configuration for tool installations,
remote hosts and verifier work roots. Keep tests independent of other checkouts.

Add a byte-scanning tracked-file checker to root preflight, with worktree and
committed/indexed modes. Code, tools, fixtures and data must contain neither the
agent messaging executable token nor absolute home-root paths. Known private
host labels in live tools are replaced with explicit inputs; ordinary identifiers
such as a nanosecond field are unaffected.

Historical plans and probes remain untouched. Grandfather only their exact
(path, Git blob hash) pairs in a committed manifest. A changed or new document
must be clean; an unlisted historical violation fails. This does not relax the
commit checker's frozen-plans rule or its one-implementation limit.

Active fixture provenance is redacted through checked metadata-only substitutions.
A helper verifies old pins and unchanged semantic fields/evidence content hashes,
then regenerates affected proof digests and embedded snapshots. Do not hand-edit
hashes or rewrite Git history. Test-only path examples retain their test meaning.

Acceptance:
- hygiene checker and isolated negative controls pass, including new plans,
  changed grandfathered documents and unlisted historical violations;
- full affected Python tool suites, fixture contracts, native manifest checks,
  census/enrollment verification and cross pass;
- root commit checker passes the signed plan/implementation pair unchanged;
- production runtime source diff is empty, with exact test-fixture and snapshot
  exceptions documented separately; no performance gates are needed;
- setup/runtime failures still persist FAIL receipts and exit nonzero without
  any messaging executable or sibling repository.

No bitmap optimization is part of this record. The read-only intersection study
found no mask-pattern branch pathology and did not justify a broad 5% gain.
