# rnx 0177: retained caller-edge feasibility evidence

**Proposed result: FEASIBILITY STOP, for Claude's closure review.** The read-only step identifies changed compiled return/drop edges and a separate context-construction comparison edge, but does not bind them to a concrete fast-path ownership callsite that can be restructured under this plan. No engine source was edited, no candidate built, no interpreter subject executed, and no PMU/wall or new disassembly collected. The current range approach remains unadopted. The default profile and frozen gates remain unchanged.

## Method and receipts

Plan `1a4a2f9` is accepted `beff0ee` amended only with Claude's requested competing CGU-partition/layout explanation and its feasibility-stop condition. Starting product/evidence commits are rnx `69222e2` and bench `b66d596c`; fork main remains `bb8e69372353c50e271c9f115bc771c77aa6b83e`. Source baseline and the 0176 candidate remain eaa59fc2 and 6f54bd32, as in the plan.

Bench tooling `cc13e84b`, `probes/range-callsite-0177/edges.py`, parses only the retained 0176 Callgrind files. Seven fixtures run before opening real profiles: a positive caller/callee graph (exclusive and inclusive costs distinct), invalid event columns, mismatched summary, negative cost, renamed compressed identity, missing edge cost, and distinct compressed identities with the same demangled name. It resolves compressed function/object/file names, counts call edges, separates edge-inclusive Ir from exclusive function Ir, and requires the sum of exclusive Ir to equal each recorded program summary exactly. No threshold is used to omit callers. Unsupported format or unresolved identities fails, rather than implying an absent edge.

The retained clean-tree analysis ran the committed tool once on eight files: baseline/candidate × context/fib/calls/numeric. Preliminary exploratory parser output was temporary and unretained; it executed no subjects and changed no measurements. Python argv/version/binary hash, fixed safe environment, status, source identity, stderr and output hashes are retained. Every compressed/raw input hash is recorded. `results/range-callsite-0177/analysis1/` at bench `dadf1477` contains `edges.json`, `identity.json`, `receipt.json` and empty stderr. All eight exclusive sums equal the independently audited 0176 totals. No inherited environment is serialized.

Profile compressed function IDs are local identities, not unique machine addresses. Multiple nm symbols have the same demangled name (22 Repr drop symbols in each binary). The report deliberately does not attach one guessed address to those edges. Full retained nm and Vm::run disassembly hashes and selected direct-call lines accompany the report; no regenerated disassembly or differently built binary substitutes for the original. All reported Ir is Callgrind instrumented instruction events, not native instructions:u. Inclusive edge costs must not be added to exclusive totals or overlapping/recursive edge costs.

## Observed caller edges

The following rows compare the retained **same named caller → target** edges; ambiguous target machine identities remain unclaimed. Calls/Ir are each profile's counts, not an assertion that source-level destruction was added or removed.

| Profile and edge | Baseline calls / inclusive edge Ir | 0176 calls / inclusive edge Ir |
| --- | ---: | ---: |
| fib: pop_call_frame → Value drop | 5,084,978 / 45,764,802 | 5,084,978 / 66,104,714 |
| fib: pop_call_frame → Repr drop | no recorded edge | 2,542,488 / 45,764,784 |
| calls: pop_call_frame → Value drop | 6,000,012 / 54,000,108 | 6,000,012 / 78,000,156 |
| calls: pop_call_frame → Repr drop | no recorded edge | 3,000,005 / 54,000,090 |
| context: BTreeMap::entry_with → infallible_cmp<Component> | no recorded edge | 30,158 / 1,498,680 |

The Repr target's exclusive Ir equals its sole incoming edge Ir in each shown candidate profile; the Value target has many callers. Fib pop_call_frame exclusive Ir changes 98,521,475 → 102,335,197, calls 134,000,241 → 138,000,239. B-tree comparison exclusive Ir is 840,142 on candidate context. These figures locate recorded work at compiled function boundaries. They do not mean the base skipped destruction/comparison; that work may be inlined or represented differently.

No try_range_dispatch node or executed edge exists in the candidate context, fib or calls profiles. Thus none of those observed callers is reachable from an **executed** fast-path node in those profiles. This is a dynamic-profile statement, not proof that the static program has no fast-path caller or that compiler partitioning caused the differences.

The frozen numeric positive control does record try_range_dispatch (profile ID 5642), and its executed descendants include `Value::into_mut<RangeIter<i64>>` and Worklist::dismantle, both callers of Value drop. So the analysis is not simply missing all helper edges. However, its Repr drop edge is again from pop_call_frame (8 calls / 144 Ir), not from the helper. The already existing normal native-conversion path also calls Value drop in the baseline. These broad, same-named ownership operations do not establish a new unique static caller whose removal would restore unrelated hot code generation.

## Symbol/source checks and competing explanation

Retained nm identifies pop_call_frame uniquely in each binary: base address 0x3da540, 772 bytes; candidate 0x330640, 756 bytes. Retained Vm::run objdump directly calls those addresses. Candidate Vm::run calls try_range_dispatch at 0x3311b0 from two sites; baseline has no such helper. Candidate infallible_cmp<Component> has a separate 102-byte symbol at 0x472960; base has no exact corresponding standalone symbol. Repr has multiple same-named symbols in both. No retained disassembly of pop_call_frame itself exists in the 0176 diagnostic, so we cannot identify its individual Repr target instruction/address from these receipts. Source describes possible ownership boundaries, not the missing machine identity.

The source change in 0176 is confined to its range helper/carrier and dispatch arms. pop_call_frame and context construction were not edited. Observed changed edges under these unrelated routines, including context construction where no range path executes, are **consistent with** CGU repartitioning, inlining visibility or layout effects. The 0174 observation that its startup pattern disappeared at cgu=1 is supporting context, not proof of this binary's assignments. No nightly CGU dump, new compiler run, profile-paired candidate, forced-inline routine or global directive is introduced. A static caller change can influence compilation without executing, so absence of a helper in fib/calls is not a refutation of every possible caller-driven mechanism either.

## Decision and next boundary

Step 0 identifies where compiled edge costs differ, but not the concrete fast-path-to-hot-drop machine identity and semantics-preserving caller transformation required to proceed. Guessing a source restructure from an ambiguous drop name would violate the feasibility checkpoint. Therefore **FEASIBILITY STOP**, with no Step 1 or Step 2 and no invented candidate/performance decision. There are zero new subject executions, builds, timing samples or engine branches in this record.

The current data justify investigating compilation partition/layout policy separately; they do not prove a remedy or justify relaxing regression gates. Such a follow-up requires the user's input as agreed. Publication after reviewer acceptance is analysis and generic evidence only, preserving both source/product mains and the named stopped 0176 branch. No upstream filing, adoption, gate change or shipped dependency change. This negative feasibility result leaves the measured allocation finding intact.
