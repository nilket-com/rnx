# rnx 0048: a bounded transport before a notebook

Status: proposed 2026-09-15. The forty-eighth record. It authorizes a
standalone transport prototype and evidence, not a kernel implementation or
an automatic replacement of record 0047's transport decision. Review of these
results comes before integration. Record 0047 remains stopped.

## Context

Two transports failed 0047's receive-memory gate. zeromq 0.6.0 reserves from
an unchecked declared frame length. The libzmq replacement caps individual
frames but buffers unfinished multipart input before exposing any part to
application code. In rnx-bench 0637193, 32 MiB of legal incomplete parts grew
RSS by about 32.5 MiB with zero application-visible parts. This is a finite
counterexample to the proposed bound, not a claim that exhaustion cannot crash
a process. It also does not establish properties of every shipping kernel.

Owning the small transport surface required here is a scope and maintenance
choice. A patched existing crate remains an alternative that could enforce
bounds. Accepting libzmq's local-peer memory exposure remains another possible
decision, but is not silently substituted for a bound. This record prices the
owned implementation by building and testing a prototype first. No line-count
or delivery-date estimate is an acceptance argument.

## Decision

### 1. A prototype outside rnx's build

Put an independent Rust crate in rnx-bench `probes/jupyter-zmtp`, with its own
lockfile. Use ordinary asynchronous TCP primitives and bounded queues; no Rune,
libzmq or zeromq dependency in this server. Reuse the pinned Python environment
at `probes/jupyter-transport/.venv` and preserve exact commands and results under
`results/jupyter-zmtp-0048`. Do not change the installed kernelspec or production
worker. Native compilation remains a client-fixture concern, not a server
requirement. Resolve, lock and record dependencies, licences and features.

The prototype exposes only the five listening numeric-loopback TCP endpoints
required by 0047: three ROUTER-shaped endpoints for shell/control/stdin, PUB for
IOPub, and REP for heartbeat. It is not a general ZeroMQ library. Support DEALER
clients on the ROUTER endpoints, SUB on PUB and REQ on heartbeat; refuse other
socket pairings. Supporting more otherwise valid pairings requires a later
scope decision. Signed Jupyter echo exchanges exercise the transport without
implementing execute, worker supervision or notebook state.

### 2. ZMTP 3.0 deliberately

The normative references are [RFC 23, ZMTP 3.0](https://rfc.zeromq.org/spec/23/)
and [RFC 37, ZMTP 3.1](https://rfc.zeromq.org/spec/37/). Implement the former's
NULL handshake and framing; 3.1 commands are deferred. Send the 64-byte 3.0
greeting, use NULL with as-server zero, send READY without waiting for the
peer's READY, and negotiate 3.0 with peers advertising 3.0 or higher. Refuse
older versions and non-NULL mechanisms. Ignore greeting padding as specified.

READY metadata names are case-insensitive. Require a compatible Socket-Type;
accept bounded unknown properties, refuse duplicate names and invalid lengths.
Support short and long frames, empty frames and binary payloads. Reject invalid
flags and truncated commands. Deliver complete messages atomically to the
application, while enforcing limits incrementally during assembly.

SUB subscriptions use the 3.0 one-byte subscribe/unsubscribe prefix and binary
prefix matching on the first publication frame, including the empty prefix.
Maintain reference counts, with bounded counters; refuse invalid subscription
messages and counter overflow. No PING/PONG generation or 3.1 command support
is promised. Unsupported post-handshake commands close that connection.
Jupyter's separate REP heartbeat remains supported; it is not ZMTP heartbeating.

These are interoperability gates, not assumptions that clients follow our
intent. The pinned Python environment reports pyzmq 27.2.0 and libzmq **4.3.5**;
4.3.4 was the previous Rust server's bundled version. Capture actual greeting,
READY and subscription traffic, including a client configured with ZMTP
heartbeats, to establish downgrade behavior. If required clients send commands
outside this scope, stop and revise rather than silently ignore them.

NULL provides no transport authentication. Jupyter HMAC verification remains
above transport framing, before dispatch; frame admission therefore cannot
rely on a signature to protect its own memory. Identity is a routing name, not
an authenticated client identity.

### 3. Routes belong to connections

Honor a nonempty READY Identity of at most 255 bytes on a DEALER connection.
Absent or empty Identity receives an opaque generated identity. Keep a
socket-local registry, detect collisions against both declared and generated
identities, and choose another generated identity on collision. On exhaustion
of the generation counter, refuse new connections rather than reuse a route.

A second live connection declaring an occupied identity is closed; do not
replace the first. Once EOF or explicit close retires the old connection, a new
connection may use its name. Tests wait for observed retirement before asserting
successful reuse, and separately test the overlapping case. Registries are
independent across the three ROUTER endpoints.

Every received envelope carries an internal connection generation alongside its
routing bytes. Replies target that generation, so an old queued reply is refused
if the connection vanished, even when a new peer owns the same name. Preserve
additional application routing frames byte-exact; do not confuse them with the
transport's connection identifier. Heartbeat consumes and restores the REQ
routing envelope, echoing the body byte-exact within the message bounds.

### 4. Limits apply before growth

These are fixed prototype policies, subject to a measured later revision:

| Resource | Limit | On reaching or exceeding it |
| --- | --- | --- |
| Connections per listening endpoint, including incomplete handshakes | 8 | close newly accepted excess connection before spawning a handler |
| Greeting plus handshake | 2 s absolute from accept | close peer |
| READY/command frame body | 8 KiB, at most 64 metadata properties | close peer before allocating oversized body |
| Incoming data frame body | 1 MiB | close peer from length header alone |
| Multipart payload per connection | 1 MiB total, at most 32 wire parts | close peer before reserving the exceeding part |
| In-progress message assembly | 5 s from its first frame header | close peer; progress does not restart the timer |
| Retained incoming payload per endpoint, including handoff queues | 8 MiB | pause admission within existing credit; no uncharged queue |
| Pending complete incoming messages per endpoint | 64 | stop admitting complete messages until capacity returns |
| SUB state per connection | 128 distinct prefixes, 256 bytes each; reference counts at most 65535 | close peer on excess |
| Pending encoded publications per subscriber, including current write | 2 MiB, 64 messages | close that slow subscriber |
| Encoded publication obligations across all subscribers, including current writes | 16 MiB | refuse publication admission without a partial fanout |
| Non-PUB pending replies per connection | 1 MiB, 32 messages | fail that connection's send admission |
| One outgoing message write | 5 s absolute once writing starts | close connection |
| Shutdown and handler joins | 5 s overall | report failure; no successful shutdown claim |

A maximum value is allowed; an excess is refused. Length conversions and sums
are checked before reservation. Charge allocated capacity, not just populated
length; vector growth, frame descriptors, metadata and queue entries need stated
bounds too. A small fixed read scratch buffer (at most 8 KiB per connection)
never grows from a peer's declared length. A connection holds at most one message
under assembly. When a message moves to a queue, its credit moves with it and is
released only when the final owner drops it. Frame-count limits also cover empty
parts, so zero-length input cannot bypass byte limits through metadata growth.

Keep endpoint receive budgets separate: shell traffic cannot consume control's
allowance. With eight connections on each of five endpoints, the receive payload
allowances sum to at most 40 MiB. Other buffers and bounded overhead are additional;
this is not a 40 MiB total-process RSS promise. Similarly, charge each subscriber's
publication obligation even if immutable bytes are shared, including partial
writes; do not count a shared buffer once to disguise unbounded queue references.

The connection cap bounds admitted handlers, not SYN backlog memory or connection
attempt rate. Account for the accept loop's single transient excess socket and
close it promptly. Fixed OS socket buffers and runtime allocations are reported
separately. Slot exhaustion by a local connector can refuse legitimate clients;
NULL plus finite slots does not promise availability against that connector.

### 5. Cancellation, fairness and publication

Each connection has independent parsing and writing state. No socket-wide reader
waits for one peer to finish a multipart. Keep completed-message scheduling fair
across peers; processing loops must yield after bounded work. Heartbeat/control
must progress while shell peers send incomplete messages or PUB peers stop reading.
Transport-limit failures close only the responsible TCP connection and free its
credits. This gives the prototype an explicit peer-abandonment mechanism.

A publication's recipient set is the matching subscriptions at admission.
Preflight all required queue credits before admitting any recipient. A subscriber
over its own limit is closed and removed; retry the remaining recipient set
without letting that subscriber stall it. If the remaining total cannot be
reserved, fail the publication operation explicitly. No subscriber receives half
a newly admitted multipart followed by the next message: a cancelled partial
write closes the connection. Other recipients continue independently.

Successful publication means bounded local acceptance, not frontend delivery.
The application can distinguish admission failure and connection closure, but
cannot know what a disconnected subscriber displayed. Empty recipient sets are
successful with no delivery claim. Reconnection requires fresh subscriptions.
No synchronous writer loop waits serially for every subscriber to flush.

### 6. Evidence before adoption

Expose counters for reserved capacities, live connections, message/queue entries,
subscription storage and released credits. Counter assertions need independent
allocation/RSS observations and source review so they cannot merely repeat an
incorrect accounting model. Run adversarial fixtures in externally limited
processes; use finite counterexamples, not deliberate exhaustion. Record sample
resolution and distinguish allocator reservations, RSS and OS socket buffers.

Property/generated tests cover parser fragmentation and coalescing; every header
split, malformed length and truncated state gets a deterministic gate. The test
client includes both raw TCP/ZMTP and real pyzmq so two copies of our own parser
cannot agree on the same mistake. Preserve real wire captures for analysis,
using a fixture HMAC key only. A timeout must fail the fixture, not skip it.

No source is copied from the reference implementations under this record. Any
later copying needs its original licence/attribution recorded. The evidence
includes clean-build cost, binary size, dependency graph, throughput and latency
under the stated workloads; none is advertised as a kernel startup measurement.

## Gates

1. Exercise each numerical bound at its maximum and just beyond, including many
   empty MORE parts, a huge length with no body, partial handshakes, slow trickles,
   eight simultaneous connections plus a ninth, and aggregate subscriber pressure.
   Show rejection before exceeding accounted capacity and release after close.
   Repeat the previous 32 MiB unfinished-multipart attempt: the peer must be
   disconnected at the bounded threshold, with no native-style accumulation.
   Stop on unexplained retained growth or unbounded allocation/queue ownership.
2. Real Python signed exchanges on shell/control/stdin, extra routing envelopes,
   wrong-signature silence, empty-key mode and binary heartbeat round trips.
   Capture 3.0 negotiation and subscription traffic from the pinned 3.1-capable
   client, with heartbeat options enabled. Test NULL/READY failures, version
   mismatch, unknown metadata, invalid flags and all header fragmentation points.
3. Declared and anonymous identities, distinct anonymous peers, live duplicate
   refusal, disconnect/reconnect under the same name and a stale queued reply.
   Force generated-identity collision through test injection. Exercise separate
   endpoint registries and additional routing frames. No cross-generation delivery.
4. Empty/nonempty prefix subscriptions, overlap, repeated subscribe/unsubscribe,
   and disconnect cleanup. A reading subscriber keeps progressing beside a
   stalled one. Control and heartbeat answer within 500 ms during finite floods
   and incomplete assembly; fixture deadlines are 5 s. Report observed timings.
   Test publication admission failure, partial-write cancellation, saturation,
   and shutdown with every parser/writer pending; all task/credit counts return
   to baseline after cleanup. No browser-delivery claim from successful sends.
5. Reproduce on Linux with preserved source, lockfiles, commands, raw results and
   source/allocation review. Type-check Windows if available and state actual
   execution separately; it is not passed by removing a native dependency.
   Review the evidence before choosing this transport in 0047. Failure leaves
   0047 stopped and the patched-crate/explicit-exposure alternatives open.

## Deferred

Kernel integration and JupyterLab screenshots remain 0047's work. General ZeroMQ
socket compatibility, connecting instead of listening, remote endpoints,
PLAIN/CURVE, ZMTP 3.1 commands, transport authentication, automatic retries and
arbitrary process-memory/availability guarantees are outside this prototype.
Linux parent-death changes remain a separate shared-rnx plan; containment is not
reimplemented here. A passing prototype earns an integration decision, not a
claim that its remaining protocol surface has been exhaustively audited.
