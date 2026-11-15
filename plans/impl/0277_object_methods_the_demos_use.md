# Implementation plan 0277: the object methods the project's own scripts use

Status: Implemented

Paired ADR: ../adr/0277_object_methods_the_demos_use.md.

1. Pin 09c47eeb. Read how method calls are resolved and certified in
   the new engine's lowering, which methods on objects, vectors,
   strings, tuples and Option are admitted today and how receiver
   types are proved, the legacy implementations of the refused ones,
   and the fixed-route inventory.
2. Before editing, send a note with the measured set as the ADR asks
   (script, call, receiver type, legacy semantics, proposed route),
   which members need a new run-time operation and are cut, the
   refusals `journey.rn` meets after the method calls if any, the
   cost on ordinary lowering, and every inventoried item touched, by
   name. If any is touched, send the note and stop.
3. Implement the design.
4. Add the controls of the ADR and regenerate the pinned tables.
5. Run both format checks, every lint recipe and the full root suite
   by hand; then the fixed-route screen, Miri, a required final
   (complete if gate tooling or census code changes), preflight and
   the commit checker. Bring the packet with the class of each
   changed row, totals before and after and the per-stage durations.
   Results state what the code does; no paths under target/. The
   candidate is frozen once the packet is sent.

## Results

The measured refusal set is one name, `get`, at three sites in
`journey.rn`: event id, event terminal flag and cell name. Continuing
compilation past each original refusal finds no further refusal. The
three stock demo scripts already admit; adapter methods are outside
this record.

Object, Vec, Tuple (including unit) and String have separate closed
provider certificates. Vec and Tuple accept integer indices and the
six stock range forms; a range returns a new Vec sharing its elements.
String accepts ranges and returns an independent String. Missing keys,
out-of-bounds ranges, reversed bounds, non-UTF-8 boundaries and an
inclusive end that cannot be advanced return None. Invalid indices
keep their typed errors. Receiver access precedes index conversion;
reading an element retains its handle without reading its payload.
Failed slice or Option admission releases acquired ownership.

An unproved receiver uses the existing dynamic method table only after
all builtin, iterator, type-token, source and Context receiver identities
have a certified provider or an absence proof. Uncertified named host
providers or unknown script implementations keep the call refused
before effects. Proven Object sites keep their direct intrinsic and
one-frame requirement. No inventoried executor item or policy changes.

The bound stock-context census moves four Method refusals to admission:
0007, 0072, 0156 and 0157. Admissions rise from 87 to 91 of the 92
inputs accepted by stock; 0212, an async file-main form, is the only
remaining refusal. The executed comparison count rises from 67 to 71.
The real journey body runs against the same fixed, isolated protocol
fixture on both engines: 19 acknowledged actions, the expected digest,
identical stdout/stderr and identical generated files. The wrapper 0072
and Polars-helper wrappers 0156/0157 execute declarations only and
return unit; they do not show any adapter executing on this engine.
The fixture stands in for the notebook application, never controls a
real application, and fails on an unexpected command or sequence.

All 242 Object-method contract rows were observed beside live stock.
Thirty-five newly admit: 28 compare exactly and seven retain the
existing D1 named missing-method wording. Preexisting Exact rows and
remaining refusal outcomes are unchanged. The per-record overlay also
refreshes seven stale diagnostics to the structured HostNotEnrolled
form introduced in 0247, preserving refusal kind, raw error and span.
Historical fixtures are untouched. Effective classes are now 186
Exact, 38 refusal and 18 D1. The live 242-row contract passes with the
new overlay. Registry policy adds get only for String, Vec and Tuple.

The separate 39-provider observation has 12 equal outcomes, 20 D1
wording differences and seven Unknown-to-Script origin enrichments;
those seven keep class, message, arguments and location. The initial
design capture's invalid clock token was replaced by paired captures
requiring the valid clock and lifecycle predicates.

Ordinary lowering frames do not grow in the matched unoptimized
verifier build: call and expr have zero byte delta; the cold
call_object_method frame decreases by 48 bytes. This measures fixed
callee-owned prologues, not the transitive call stack. Element and
boundary checks in the three new providers do not scan the receiver.
Object get, proved or dispatched, retains the existing linear field
scan against stock's hash lookup. This record extends that existing
route to more call sites; indexed Object lookup and construction cost
are an explicit follow-up, not a constant-time claim here.

VecDeque is installed by the default contexts, but its Rust get is
registered only as INDEX_GET. A control verifies the named get key is
absent, then installs a real named get and requires compilation to
refuse an unproved call. Source registration inspection finds named
get providers in Polars and Candle; those uncertified adapter providers
also defeat this general proof. No adapter route is added.

Before the first implementation was signed, the 35 native recipes passed. The complete Rune
feature/doc matrix, all four Rune Clippy recipes and format passed
after the three-provider registry refresh. An untracked rehearsal
runner's shared output-directory collision was fixed by isolating the
explicit contract output. The extra live contract was then rerun in
full after its seven inherited diagnostic pins were refreshed. Runtime
code did not change during these test-policy repairs.

The first fixed-route screen exceeded the per-record native shared-rows
level band: 294094 to 298943 program/unknown instructions at the larger
size, with its growth and every cumulative check passing. Attribution
found extra outlined calls inside unchanged native compilation routines.
An out-of-line annotation on the new provider resolver did not restore
the band and was discarded. Inline hints on BVec::push and
BVec::with_capacity restore the native compiler boundary, without
changing their allocation or budget semantics. In the isolated paired
observation, shared-rows program/unknown counts become 183572 and
291440 at the two sizes (baseline 186226 and 294094); raw totals become
192823 and 304315 (baseline 195477 and 306969). Both metrics improve
by exactly 2654 at each size. The source inventory and band limits
remain unchanged.

The admission overlay has an explicit live-consumer declaration in the
data policy, naming the contract loader and its exact byte ceiling.
