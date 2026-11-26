# ADR 0346: Gate rules for declared and loader changes

Status: Proposed

Base: published 6336eb2a (ADR 0345).

## Problem

The record that makes host functions values was stopped twice by
the fixed-route gate, each time by a rule and not by a cost.

The source condition compares the canonical text of every item in
the policy's inventory between the base and the candidate, and
fails on any difference. It has no way to accept a change that a
record makes on purpose, and every record since ADR 0296 has left
the items alone. Host function values change two of them, the
executor's `fixed_intrinsic` and `call_closure`. Generators and
async calls will change more.

The setup phase of a T row on the installed image counts the
instructions of the dynamic loader together with the product's.
The `shared_rows` row with native code on has a frozen setup
allowance of 11 instructions. A rule of ADR 0337 accepts up to
four added relative relocations at 12 instructions each. On two
different builds of the host function work the row rose by 138 at
both sizes: 120 in the loader's relocation loop, for ten added
relocations, and 18 in the loader's `strcmp` during symbol lookup,
with the same 242 calls on both images. Nothing in the product's
own setup moved. The execution rows and all 32 cumulative rows
passed. A static table of ten pointers fails the row, and so does
a shift of the dynamic string table.

## Decision

1. The policy carries a list of declared source transitions. An
   entry names one inventory item, the sha256 of its canonical
   text before and after, the base commit it applies to and the
   ADR that makes the change. The source condition accepts a
   changed item only when an entry matches the item, both hashes
   and the baseline of the comparison. An entry for another base
   accepts nothing. An entry for an item that didn't change is an
   error. The receipt lists each accepted transition.
2. In the setup phase of a T row on the installed image, the
   dynamic loader's instructions are scored apart from the
   product's. The loader's are the instructions of every function
   of the loader object the policy names by path and digest, read
   from the profile the row is scored on, on both images. The
   rest of the row's setup change stays under the row's frozen
   allowance. The loader's change is held to the gate's per-record
   limit, applied to the loader's own instructions on the base
   image. This replaces the count of added relocations as the
   condition. The count is still reported.
3. If the loader's digest isn't the policy's, or a profile lacks
   the loader, the ordinary rule applies alone.
4. Nothing else in the gate changes: the inventory, the kernels,
   modes and sizes, both per-mille limits, the frozen allowances,
   the cumulative rows and the checks against a weaker policy.
5. This record declares no transition and changes no product
   source. Record the count of legacy-only cases on the base and
   on the candidate. Both are 70.

## Controls

1. Source condition: a changed item with no entry; a wrong old
   hash; a wrong new hash; an entry bound to another base; an
   entry for an unchanged item. Each fails. A matching entry
   passes and is listed.
2. Loader rule: a setup rise wholly inside the loader and inside
   its limit passes. A rise of the product's own setup over the
   frozen allowance fails with the loader unchanged. A loader rise
   over its limit fails. A changed loader digest and a profile
   without the loader fall to the ordinary rule.
3. The two measured pairs of the Problem are scored again under
   the new rule from their kept profiles. Results give the
   loader's instructions on each image, its limit and the
   remainder.
4. The policy file differs from the base only by what Decisions 1
   and 2 need. Results list the changed fields.
5. Before signing, the whole final-controls stage by its stage
   command, with the pinned controls list brought up to date.
   Then one complete final on the signed candidate, under the
   limits of ADR 0338.

## Constraints

- No new third-party crate. Tabs.
- No limit is raised and no row is excused by name.
- The product's sources under `rune/` and `src/` are unchanged.
