# rnx 0048 evidence: the bounded heartbeat extension passes

Measured by Codex on nano/Linux on 2026-09-15. Plan `1f689be` preceded the
prototype. Sources, lockfile, tests, wire traces and environment/build data are
in rnx-bench **ae74699**, `probes/jupyter-zmtp` and
`results/jupyter-zmtp-0048/README.md`. This is a prototype, not a kernel.

## Original stop condition

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


## Accepted extension and completed prototype gates

Measured 2026-09-15 after plan revision `29f6fda`. Current implementation and
results are in rnx-bench **00fc828**, `probes/jupyter-zmtp` and
`results/jupyter-zmtp-0048-extension/README.md`. The earlier sections describe
the original failed candidate; the extension supersedes their pending decision.

PING accepts 7–23 body bytes and PONG 5–21, including the length byte and name.
Context is at most 16 bytes and TTL is two bytes. That corrects the proposed
22-byte-after-name wording before implementation. PONG uses the same bounded
connection writer. Valid inbound PONG and TTL are ignored. Heartbeats do not
reset a multipart deadline or impose a five-second lifetime on an idle connection.
Other 3.1 commands remain unsupported and the greeting still advertises 3.0.

Both complete wire-fixture passes exit zero with client heartbeats enabled.
Eleven Rust tests pass, including exact/excess capacities, subscription counters,
atomic fanout failure, current-plus-queued write credit, fragmented/truncated
frames and the five-second writer deadline despite partial progress. Formatting
and the Windows MSVC type check pass. No Windows execution is inferred.

The paced PUB gate sends 1000 messages with a requested 2 ms producer pause.
The stalled subscriber retires, the reader keeps generation 32, and all 1000
messages plus a final marker arrive. Control calls during pending publication
remain below the 500 ms gate (at most 0.994 ms in confirmation). This is continuity
under the stated load, not arbitrary-rate lossless delivery. The earlier unpaced
flood is retained as an inconclusive result rather than counted as this gate.

The aggregate wire test occupies all forty slots with 40 MiB of partial payloads,
refuses excess connections and repeats cleanup three times. Confirmation live
requested allocation returns to 116696 bytes after each cycle; while full it
is about 42.3 million bytes, with the largest request exactly 1 MiB. RSS is
sampled separately. This is neither a 40 MiB process ceiling nor a measurement
of allocator-internal size classes. Independent System-allocator counters,
wire traces and the source ownership accounting are preserved together.

Two completion findings were corrected. Payload counters and allocator totals
were sampled at different instants, so the fixture now awaits both quiescence
conditions within its unchanged five-second bound. The failed early assertion
is preserved. Also, connection permits previously could drop before completed
JoinSet records were collected. Permits now stay in a listener-owned bounded
map until joining, bounding completed metadata as well as running connections.

A blocked 250499-byte reply is not delivered to a reconnected peer with the same
identity. Shutdown of a blocked writer completes in about 4.4 ms in confirmation.
Other gates establish greeting, READY, header, body and multipart pending states
before stopping. Final snapshots show zero connection/route/payload/publication
credits. The 64-message inbound queue ceiling is satisfied structurally: the
prototype has no additional input queue and retains at most one message per
connection, eight per endpoint. Integration must preserve its ownership bounds
if it introduces another queue.

The evidence map covers the remaining numerical endpoints and failure modes.
The clean offline build took 5.35 seconds; local signed-echo samples averaged
about 0.433 ms in confirmation. These are fixture observations, not kernel costs
or a speedup claim. Source/binary hashes, raw exits, versions, package licence
declarations and the complete reproduction script are committed.

No root-rnx source or dependency changed, so its full suites were not rerun.
There is no kernel installation, JupyterLab capture or production process change.
0048 is ready for review; adoption and notebook acceptance remain 0047's work.
