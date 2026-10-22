# rnx 0190 evidence

Tool: cargo-audit 0.22.2 on strix. Advisory-db commit 640bf5a9
(2026-10-22T12:49:47+02:00), used for every check below.

## Before (HEAD 2f1a9ca, after 0189)

The root lockfile matched 9 vulnerabilities: rustls 0.23.44 (RUSTSEC-2026-0285,
-0346, -0347, -0348, -0349), quinn-proto 0.11.17 (-0338, -0340, -0343) and
rustls-webpki 0.103.15 (-0382). adapters/postgres, servers/http-postgres,
demos/polars and demos/candle matched the same set. adapters/polars and
adapters/candle matched the rustls 0.23.45 subset (-0346 to -0349) and -0382.

## After

| Lockfile | Remaining vulnerabilities |
| --- | --- |
| root | none |
| adapters/postgres | none |
| servers/http-postgres | none |
| adapters/polars | quick-xml RUSTSEC-2026-0194, -0195 |
| demos/polars | quick-xml RUSTSEC-2026-0194, -0195 |
| adapters/candle | zip RUSTSEC-2026-0367, -0368, -0369 |
| demos/candle | quick-xml -0194, -0195; zip -0367, -0368, -0369 |

Package changes per lockfile (parsed TOML): rustls -> 0.23.46,
rustls-webpki -> 0.103.16 and quinn-proto -> 0.11.19 (not present in
adapters/candle). No other package was added, removed or changed version. Two
kinds of same-version dependency edges changed, both compatible
deduplications by the resolver:
- Windows-only `windows-sys` 0.52.0 edges now point to 0.61.2, in every
  lockfile.
- In adapters/candle and demos/candle, `tempfile` 3.27.0 now depends on
  `getrandom` 0.3.4 instead of 0.4.3. This edge is not Windows-only. tempfile
  declares `getrandom >=0.3.0, <0.5`, and 0.3.4 was already in both lockfiles,
  so the resolver collapsed onto it.

Remaining paths, both semver-major upstream moves:
- quick-xml 0.39.4 comes in through object_store 0.13.2 (polars 0.55.2); the
  fix needs object_store with quick-xml >= 0.41.
- zip 8.6.0 comes in through candle-core 0.11.0. The three advisories concern
  `extract`; candle-core reads archive entries (npz/pth) and does not extract
  them to disk. This is a risk note, not an exemption; the dependency move
  remains a follow-up.

## Behavior

Release `rnx --locked` with this change: the eMudhra check hosts dv-g3b,
ov-g3c, ev-g3b and dv-g3b-aaa are refused, and www.mozilla.org returns 200.

Root suite (strix, `--locked --no-fail-fast`): 51 test binaries, 478 passed,
0 failed, 2 ignored. All four third-party notices were regenerated; each
passes `--check`.
