# ADR 0322: The engine image ships as a package

Status: Proposed

Base: published 3e4844eaa0155ceeabd4af5f42e743cc9fda91c2 (the column NaN fix after ADR 0321).

## Problem

Records 0317 to 0321 built a separately linked engine image that runs
checked files and the file adapters, measured against stock. It is still
a test arrangement:
- The launcher, `build-adapter-launcher.sh` and the provider live under
  `demos/` and `engine/image/tests`. The launcher links `registry.c` from
  a demo and needs the `stock_file_inventory` test oracle to generate a
  header.
- No ABI runs an ordinary stock file completely. v1 accepts no callback
  and uses the real loader and runner, but it passes an empty argument
  list, omits the product installers (`install_core`, env with script
  arguments, fs, path, time, text, HTTP lifecycle, interrupt), collapses
  every nonzero exit to one status, and returns diagnostics only after a
  one-shot call, so growing the buffer would repeat effects. v2 and v3
  pass real arguments and load file modules, but they reject a null
  provider at open and an ordinary program at compile.
- The provider implements exactly the combined Polars and Candle shape.
  A subset fails registration, and the full table for a subset would
  expose an undeclared namespace.

This is record 5 of the six isolation records. Record 6 changes the
installed default.

## Decision

1. File entries. Two additive, versioned, launcher-only entries share
   one file invocation with the production stock context: the same
   installers, `Loader`, arguments, environment, rendering and exit
   handling as the stock CLI.
   - The stock entry takes no provider.
   - The provider entry takes the existing v3 provider table and installs
     it once into that same context. The provider's `interchange::Dense`
     replaces the stock one under the same Rune name; nothing else in the
     stock surface changes. It selects the ordinary or checked program
     before any effect, so one package runs both kinds of file.
   - A script's `process::exit` ends the launcher process with its code,
     as on the stock CLI. That is why these entries are launcher-only.
   - The caller gets an outcome separate from the product exit code, so
     exit 1, interrupt 130 and pre-effect refusal stay distinct.
   - Diagnostics stream during the one invocation or are kept for
     readout. No path replays a file.
   - Standard output and error are inherited. No general I/O ABI.
   - v1, v2 and v3 layouts and exports keep working.
2. Product homes.
   - `engine/image` keeps the engine crate and the versioned C headers.
   - `engine/launcher` holds the shipped launcher's source and build
     entry.
   - The provider's owner and registration code moves once into a
     product adapter package. Demos and contract fixtures consume it.
   - Registration and refusal metadata come from the selected product
     installers. The stock inventory stays an independent test oracle.
   - Fixture-only commands (the host apply offset, tagged extension,
     trap, missing inventory and observation) stay in controls.
3. Two build entry points, both opt-in.
   - Stock packaging writes to an explicit output directory.
   - The project builder gains an opt-in package mode sharing the same
     packager. Ordinary file `run` spelling and arguments are used.
   - Today's installed CLI and ordinary generated executables keep their
     route. Default routing and fallback belong to record 6.
4. Supported sets. A package supports no native adapters (the stock
   entry) or exactly Polars and Candle (the combined provider). Every
   other set is refused before Cargo or the linker starts, with one
   stderr line and the normal nonzero build exit:
   `image package: unsupported native adapters [<sorted aliases>]; supported sets are [] and [candle, polars]`.
   A declaration whose resolved package, source or registration contract
   the packager cannot honor gets:
   `image package: unsupported native declaration for '<alias>'; expected the reviewed <package> provider`.
   Ordinary manifest validation runs first. A package name alone does
   not make a checkout trusted.
5. Entry selection. A package chooses the ordinary or checked entry
   before any effect. A pair never installs its adapters and silently
   runs an unrelated stock-only context. Project source-alias module
   mappings are either supported or declined explicitly before
   packaging.
6. Artifact contract.
   - The release engine is built once, under its own locked feature
     graph, and every launcher links those exact bytes.
   - A package is a directory holding the launcher and the adjacent
     `libengine_image.so`, resolved relative to the launcher's origin,
     with the entry identity verified before compilation or any provider
     callback.
   - The project artifact receipt and cache bind both hashes, the ABI,
     target, provider set and launch recipe.
   - Assembly happens in a fresh directory. A package is exposed only
     after both files verify. A failed rebuild leaves the last usable
     package intact.
7. Not in this record: the installed default, legacy fallback parity
   and the one-time cutover comparison (record 6). No cutover allowance
   is spent here.

## Controls

1. File entries. A frozen stock CLI witness, compared with stock: a
   sibling module, an argument containing spaces, a stock battery call,
   stdout, stderr, a normal exit and a nonzero terminal outcome. Errors
   and long diagnostics cause no duplicate effects. Pre-effect refusal
   and terminal error are distinct. Explicit `process::exit` matches
   stock, including output written just before it. The provider entry
   runs an ordinary file and a checked file in one package, and its
   stock surface differs from stock only in `interchange::Dense`.
2. Packages. From the real stock and project entry points, a scalar
   file runs from the stock package and the checked CSV-to-tensor model
   witness from a generated Polars and Candle package, checking
   arguments, output, exit and engine route. Each package is moved
   outside the checkout, the build tree is made unreachable, and the
   runs repeat.
3. Isolation. Changing the generated provider and rebuilding changes
   behavior, with the same engine SHA-256 and a different launcher
   SHA-256. The provider-only rebuild does not rebuild the engine.
4. Refusals before effects: a missing, substituted or shadowed image
   on the search path, a stale cache entry, each unsupported adapter
   set, and a correctly named pair with an overridden builder, which
   starts neither Cargo nor the linker. Installed ELF ownership is verified and
   public commands do not expose the fixture API.
5. Cost and cleanup.
   - Release wall time for the once-built engine, cold stock and
     generated launchers, an unchanged rebuild and a provider-only
     rebuild.
   - Bytes for each launcher, the engine and each package.
   - Allocated bytes before, at peak and after, in fresh owned target
     directories on the same filesystem, adapter workspaces included.
   - An unchanged rebuild retains no new artifact generation. Success,
     link failure and interruption release superseded and private build
     roots. Shared Cargo caches are not deleted to hide growth. Any
     deliberately retained cache is reported with its eviction bound.
6. Speed. The published fixed-route bands and the matched provider
   sibling comparison pass on the packaged artifacts. Stock-to-image
   setup is reported for record 6.
7. Finals. Targeted pre-sign checks, then one complete final on the
   signed candidate. A failure keeps its evidence and stops.

## Constraints

- Target: x86_64-unknown-linux-gnu. No new third-party crate. Tabs.
- ABI v1, v2 and v3 callers keep working unchanged.
- No engine arithmetic or dispatch change.
