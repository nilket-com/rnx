# rnx 0190: TLS-stack security refresh

The first advisory audit of rnx (cargo-audit 0.22.2, advisory-db 640bf5a9,
2026-10-22), run after 0189, still matched vulnerabilities in every lockfile
that ships a TLS client:

- `rustls-webpki` 0.103.15, RUSTSEC-2026-0382: certificate verification may
  accept distrusted intermediates. This is the same incident as 0189, at the
  verifier layer. Fixed in 0.103.16.
- `rustls` 0.23.44/0.23.45: RUSTSEC-2026-0285 (handshake messages accepted
  across encryption levels), -0346 (Debug impls expose session secrets),
  -0347 (application data sent after ECH rejection), -0348 (unauthenticated
  Brotli decompression up to 1 GiB) and -0349 (ServerHello downgrade). All are
  fixed in 0.23.46.
- `quinn-proto` 0.11.17/0.11.18: RUSTSEC-2026-0338, -0340 and -0343 (remote
  peer panics). Fixed in 0.11.18 or later.

All three fixes are semver-compatible within the declared requirements.

## Change

Run `cargo update -p rustls -p rustls-webpki -p quinn-proto` in the seven
active lockfiles: root, adapters/polars, adapters/candle, adapters/postgres,
servers/http-postgres, demos/polars and demos/candle. Each lockfile changes
only those three package versions, plus Windows-only `windows-sys` edges that
the resolver deduplicates. No manifest or source changes. Regenerate the four
third-party notices.

## Validation

1. cargo-audit with the same advisory-db commit: no rustls, rustls-webpki or
   quinn-proto advisory remains in any of the seven lockfiles.
2. The release `rnx --locked` build still refuses the eMudhra check hosts and
   still reaches a control host.
3. Run the root suite with `--no-fail-fast`, plus every notices `--check`.

## Out of scope

The semver-major findings need upstream dependency moves and stay open,
each as a tracked follow-up:
- `quick-xml` 0.39.4 in the polars adapter and demo (RUSTSEC-2026-0194/0195,
  fixed in 0.41);
- `zip` 8.6.0 in the candle adapter and demo (RUSTSEC-2026-0367/0368/0369,
  fixed in 9.0).

Unmaintained-crate warnings (`paste`, `bincode`) are not vulnerabilities.
An advisory gate and a daily watch, so that detection no longer depends on
the operator noticing, are a separate record.
