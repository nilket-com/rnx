# rnx 0048 evidence: the client still sends PING

Measured by Codex on nano/Linux on 2026-09-15. Plan `1f689be` preceded the
prototype. Sources, lockfile, tests, wire traces and environment/build data are
in rnx-bench **ae74699**, `probes/jupyter-zmtp` and
`results/jupyter-zmtp-0048/README.md`. This is a prototype, not a kernel.

## Stop condition

The server sends a ZMTP 3.0 greeting. The pinned Python client (pyzmq 27.2.0,
libzmq 4.3.5) sends a 3.1 greeting and completes NULL/READY. Its subscription
uses the 3.0 prefix: `0170726f6265`, subscribe to probe. Initial publications
arrive. With HEARTBEAT_IVL=100 ms it then sends command body
`0450494e470000`: PING with zero TTL and empty context.

Decision 2 deliberately excludes that command and requires closure. The server
closes the connection; the required publication gate fails in two captured runs.
This is the record's explicit compatibility stop. No unsupported command is
ignored, no PONG is fabricated, and no new protocol scope was silently added.
The next review must choose how to handle this client's command. Exit status 1
and the corresponding assertion are preserved, alongside successful cleanup.

## Independent coverage and its limits

The explicitly named diagnostic runs disable the client's transport heartbeat
option, and do not pass the failed gate. They exercise signed/empty-key exchanges,
three routing endpoints, binary REP heartbeat, declared lengths through u64::MAX,
1 MiB/1 MiB+1 payloads, 32/33 parts, metadata bounds, anonymous/declared identities,
live duplicate refusal and reconnect, and a zero-prefixed collision. The final
confirmation uses the session-id bytes for DEALER identities. Generated names
begin with zero but arbitrary raw peers may still declare those bytes, so the
collision check remains necessary.

The earlier 32 MiB unfinished message is now disconnected on the 32-part cap
for its 8 KiB parts. The sender wrote 917504 bytes into TCP before observing
closure in the confirmation; that is not server retention. RSS was sampled
before/after (4018176/4132864 bytes), not at its peak. An exhaustive independent
allocation audit remains outstanding. Counters confirm credit release on shutdown.

Five Rust tests pass, covering header splits, coalescing, truncation, capacity
release, subscription limits and preflight/closed-generation mechanics. This is
not complete coverage of all record gates. The flood delivered output but both
subscribers were closed at its final snapshot, so sustained healthy-peer progress
is unproven. Paced publication, aggregate combinations, delayed stale replies at
the connection level, every pending teardown state and throughput characterization
remain outstanding. They are not treated as passed because the diagnostic run
exits zero.

The fresh-target offline release build took 5.37 seconds. Windows MSVC target
checking passes; no Windows execution is claimed. Rust formatting and Python
undefined-name checks pass. Binary/source hashes and licence declarations are
preserved, not a finished kernel notices audit. There is no production rnx edit,
full-rnx-suite rerun, notebook installation or screen evidence under this record.

## Next

Review the bounded heartbeat-command extension versus a different supported-client
contract. Neither is chosen here. The prototype is not marked implemented or
adopted, and record 0047's integration remains stopped.
