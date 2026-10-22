# rnx 0189: drop the compromised eMudhra trust path

Eight eMudhra intermediate CAs (TLS DV/OV/EV G3B and G3C, plus two code
signing CAs) were issued with private keys that can be recovered, so anyone
can mint certificates that chain to them. Every one was revoked on
2026-10-18/19 for `ca_compromise`. Source: the incident's published inventory
(`impacted-intermediates.json` sha256 `f67b3abe…c59e`, `roots.json` sha256
`119f32cd…2380`) and Mozilla bug 2083449.

rnx verifies TLS through reqwest and rustls against the compiled-in
`webpki-roots` (Mozilla's root list). Its version 1.0.9 trusts `emSign ECC
Root CA - G3`, which cross-signs `emSign Root TLS CA - G3`, the issuer of the
TLS intermediates. That is the only path rnx trusts: the Sectigo cross-signs
chain to `AAA Certificate Services`, which Mozilla had already removed and
1.0.9 does not contain, and the emSign TLS/CS G3 roots were never in it.
`webpki-roots` 1.0.10 (2026-10-19) removes `emSign ECC Root CA - G3`, which
leaves no trusted path to any affected intermediate.

## Change

Update `webpki-roots` 1.0.9 -> 1.0.10 in all seven active lockfiles that
contain it: root, adapters/polars, adapters/candle, adapters/postgres,
servers/http-postgres, demos/polars and demos/candle. The lockfile change is
`webpki-roots` only. No manifest, source or other package changes. The
jupyter, tools/project and tools/polars-gen locks do not contain it.
Historical probe locks remain unchanged.

## Validation

1. Build the release `rnx` at HEAD and with this change, both `--locked`.
2. Fetch every TLS check host from the incident page through `http::get`.
   Hosts that chain only through the Sectigo cross-sign must fail on both
   builds. Hosts that chain through `emSign ECC Root CA - G3` must succeed on
   the old build and fail certificate verification on the new build. A
   control host with an unaffected chain must succeed on both.
3. Run the root suite.

An intermediate blocklist (a custom rustls verifier or CRLs) is not needed for
the trust store rnx ships. Reconsider it only if rnx gains a system-store or
platform-verifier route.
