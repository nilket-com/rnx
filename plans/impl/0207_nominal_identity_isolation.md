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

## Constructor and provisional-identity audit

Interner::with_limits is called in production only by nominal::with_global,
which passes its candidate to with_prepared and installs it behind GLOBAL.
Other construction sites are module tests. Make Interner private to preserve
that boundary. The private-table inequality in limits_refuse_before_publishing
must compare raw indices, because Nominal::PartialEq consults published metadata.

Staging::stage_shape/stage_variant_order can produce high-bit provisional IDs.
Transaction::stage, stage_shape and stage_variant retain them in the builder.
Transaction::finish validates through Staging::valid and constructs/audits the
decoded cache before publication. code::audit_cached delegates to same_op;
same_tag explicitly compares Struct IDs by their raw u32, bypassing Nominal
semantic equality. The frontend receiver-proof comparisons use Hash, not Nominal.
Staging lookup compares entry hashes/shapes/fields, not semantic Nominal equality.

After all fallible validation and remap allocation, finish calls staging.commit
then for_each_type_mut to replace every provisional Struct ID in encoded type
pools and cached operands. No fallible operation follows publication. Debug
assertions check that no provisional ID remains and re-audit the cache. A refused
finish returns no Code; the staging/builder values drop. Published execution
therefore cannot observe the private local-table IDs in the failing test.

Evidence: target/adr0207/reproduce-v1/RESULT.json. The fresh child forces the
publication with two Barrier rendezvous; local metadata stays distinct while
the legacy global equality assertion fails. This is not a failed-suite retry.
