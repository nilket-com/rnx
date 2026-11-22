# ADR 0324: Files run on the installed engine image

Status: Accepted

Base: published 05283812784034628fe29755c7640e352e4f1534 (ADR 0323).

## Problem

Record 0322 ships the engine image as an opt-in package. The installed
CLI still runs files on the old core. The new engine runs only through
`--image-package`, an explicit build, or the in-process path behind the
opt-in split. Until the image is the default, the old core keeps every
user and cannot be deleted.

The 0322 census admitted 92 files on both engines, found 152 that fail
on stock, and found none that only stock admits. It does not cover:
- typed refusals on real files;
- the legacy selection itself;
- argv and flags through the public CLI;
- exit and interrupt behavior through an installed binary.

v4 reduces every refusal to `RNX_REFUSED`, and the C launcher returns 1,
so a refusal cannot be told apart from a script that exits 1.

This is record 6 of the isolation path, split in two. 0324 moves files.
0325 moves projects and generated executables onto the same image,
refusal contract and evidence format.

## Decision

1. Installed default. After a clean documented install, plain
   `rnx run FILE` on x86_64-unknown-linux-gnu runs on the packaged stock
   image. There is no flag, environment toggle, checkout or build at
   launch. The public CLI front end keeps flags and non-file commands.
   The bare prompt, eval, worker, fmt, management and serve stay on the
   old runtime, and the record names them as the next blockers.
2. One executable. README's promise stands: one installed executable,
   with no companion program or runtime installation.
   - The installed CLI embeds the stock bundle. Only the CLI side embeds
     it. The image's cdylib keeps its default-features-off dependency on
     rnx, so the bundle never embeds itself.
   - On first use the bundle is materialized at a content-addressed
     path: its digest is in the name, and publication is atomic. A file
     is never written in place, and a stale generation is a different
     path.
   - The image uses initial-exec TLS (0318), so it is linked at
     process startup, not loaded late. Late `dlopen` fails when the
     static TLS surplus is small, for example with
     `glibc.rtld.nns=1`. The bundle includes a small file helper whose
     literal `DT_NEEDED` names the image as `/proc/self/fd/N`.
   - Before the CLI builds a file runtime or installs signal handlers,
     it materializes and verifies both the image and the helper. It
     checks that the cache directory is owned by the user and is not
     group- or world-writable, pins both descriptors, places the image
     at descriptor N, and executes the helper by descriptor. The helper
     marks N close-on-exec and checks the entry mapping against it. The
     existing mapped-symbol check stays as a second check. There is no
     `LD_PRELOAD`, no loader search path and no TLS model change.
   - The extra exec is part of the steady launch cost.
   - Full verification is the default. A cheaper steady-launch check is
     allowed only when it is measured and its argument is stated:
     the owner-controlled cache as the trust boundary, file identity
     including ctime, and behavior on replacement.
3. Compatibility route. `auto` moves a file to the old core only on a
   typed refusal before any script effect. The continuation happens
   inside the same image invocation, with the same loader and source
   snapshot, and emits a structured refusal event. The CLI never
   restarts the script, and macros, host callbacks or script effects
   never run twice. Explicit legacy selects its path up front.
   - Route evidence separates explicit `--engine=legacy` from automatic
     refusals by category. Ordinary output does not change.
   - Runtime errors, exits 1 and 130, and a missing, wrong or failed
     image are terminal and never fall back.
   - This route is temporary. Its deletion follows the counts from 0324
     and 0325.
4. Structured refusals. A refusal carries its category, reason and
   source location across the image boundary from the source that
   detected it. It is never recovered by running the file a second time.
   Earlier ABI callers keep their layouts and behavior.
5. Arguments. Non-UTF-8 script paths and arguments go through the
   existing `command_line` check and exit 2 before dispatch, as stock
   does. The v4 request stays UTF-8.
6. Deletions now. The opt-in/default split and the in-process new-engine
   file path are removed from normal file routing. The old core remains
   only behind the compatibility route and for non-file commands.
7. Parity is established once. A cutover packet compares the installed
   candidate against frozen 05283812 on the same isolated fixtures and
   inputs. It compares stdout, stderr, status, route and reason, and
   effect counts:
   - the census: 92 admitted by both, 152 that fail on stock, none that
     only stock admits, with differences kept per case;
   - genuine refusals by category: bytes, async and generators, and
     missing names or operations, kept apart from supported async and
     ordinary compile errors;
   - argv and flags, including empty and spaced arguments;
   - exit codes, SIGINT, and a failure after an effect;
   - the frozen large-input witness on a stock file.

   After acceptance, the dual-run comparison retires. The existing
   semantic and performance gates continue.
8. Setup cost. The one-time measured delta is accepted. Cold
   materialization and steady launches are measured separately on the
   installed path, including dispatch. The 48 stock-to-image
   observations from 0322 are archived. The installed setup becomes the
   frozen reference for later per-change and cumulative gates. Execution
   cost gets no blanket allowance.
9. Not in this record: projects, generated executables, provider
   packages, and the >2 GiB CSV route through `polars::read_csv` (0325);
   deleting the fallback and the legacy file path (the record after).

## Controls

1. Install. Install with a clean, locked `cargo install` from a clean
   clone of the candidate. It completes without recursive
   embedding or waiting on its own target lock. Move the executable to a
   clean location and hide the checkout. Then run offline twice: once
   with an empty cache and once warm.
2. Cache safety. Each of these is refused before load:
   - a cache directory owned by someone else, or one that is group- or
     world-writable;
   - a substituted or truncated file;
   - a file replaced between the check and the load;
   - a symlinked entry;
   - a substituted helper.
   Concurrent first launches publish one generation. The installed route
   runs with `glibc.rtld.nns=1`.
3. Route. A typed refusal of each category falls back once, with a
   count and no repeated effects. Explicit legacy is counted separately.
   Runtime error, exit 1, exit 130 and image failure do not fall back.
4. Refusals. Category, reason and location match what the source
   detected, and earlier ABI callers are unchanged.
5. Packet. The cutover packet in Decision 7, with differences kept per
   case.
6. Cost. Cold materialization, steady launch with full verification,
   and any cheaper check, each measured on the installed path. The
   fixed-route bands and the stock-to-image execution checks pass.
7. Finals. Targeted pre-sign checks, then one complete final on the
   signed candidate.

## Constraints

- Target: x86_64-unknown-linux-gnu. No new third-party crate. Tabs.
- ABI v1 to v4 callers keep working unchanged.
- No engine arithmetic or dispatch change.
- Non-file commands are unchanged.
