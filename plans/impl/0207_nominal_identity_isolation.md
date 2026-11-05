# Implementation plan 0207

Reproduce the local/global namespace collision in a fresh child process. Two
threads and a Barrier force private indices 0/1 to exist before another thread
publishes two global descriptors with the same semantic hash at indices 0/1.
The original inequality assertion must fail after publication, without sleeps,
iteration-count scheduling, or retry. Preserve the original suite failure.

Audit every Interner constructor and Nominal constructor. Production registry
construction runs through with_global/with_prepared; local interner construction
outside that route is confined to tests. Provisional staging indices are
resolved through the publication remap before they enter executable metadata.
Confirm this invariant before choosing the test-only identity assertion fix;
if the audit or reproducer exposes a production namespace escape, revise the
decision to separate identity types before implementation.

Compare raw indices for private-table identity assertions; retain semantic
comparisons for published global identities. Restrict Interner visibility.
Delete the library isolated-retry branch and add a control rejecting its
reintroduction. Validate the deterministic interleaving and ordinary parallel
feature suites, retain failures without retries, update exact manifest counts,
and run fresh-clone/Miri gates at the stacked 0207 tip. Publish 0206 and 0207
together only after all required gates pass.
