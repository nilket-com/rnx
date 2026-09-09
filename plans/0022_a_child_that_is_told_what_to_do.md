# rnx 0022: a child that is told what to do

Status: proposed 2026-09-09. The twenty-second record of rnx. A script can run
a child and read what it said, but it cannot say anything to it. That is why
the graft verifier runs `git cat-file` four thousand times where the original
runs it three, and this record gives a child its standard input.

## Context

Record 0021 got the port running on the whole workload and measured what it
costs: python 106 ms, the port 4,949 ms, identical output. The gap is one
thing. `verify_nilket_graft.py` holds a `git cat-file --batch` open and writes
object ids to it; the port cannot, so it spawns a child per object.

The protocol the original uses, exactly, from its `batch`:

1. Spawn `git -C repo cat-file --batch` with all three pipes.
2. Write every object id, one per line, then **close** standard input — the
   child needs the end of the stream to finish.
3. For each id, read a header line `<id> <type> <size>`, then exactly `size`
   bytes, then one newline.
4. Read standard error, and require the child to exit 0.

Measured on the 1,346-commit lineage record 0021 checked in a generator for:

| | |
| --- | --- |
| 4,038 objects as separate `git cat-file` invocations | 4.50 s |
| the same 4,038 through one `cat-file --batch` | **0.02 s** |
| the request stream, all 4,038 ids | 165,558 bytes |
| the reply stream | 534,882 bytes |

**A pipe on this machine holds 65,536 bytes.** Measured: 65,536 written to a
pipe whose reader is asleep returns; 70,000 blocks. So a request stream of
165,558 bytes cannot be delivered in one write while nothing drains the
replies.

**What that does not establish.** It does not prove this git batch hangs. I
tried to reproduce the naive sequence — write every request, then read, in one
process — under an eight second external timeout, at 41,000 and at 165,558
bytes of requests against reply streams larger than a pipe. **Both completed,
in about 105 ms.** Whatever git does with its input, it did not wedge, and the
arithmetic that says it should is not evidence that it does. The hazard is
therefore carried as a **risk** below rather than as a demonstrated failure,
and decision 2 is justified by removing a class of hazard rather than by a
reproduction.

What is still true is that the original's margin is thin and unexamined: it
batches 1,346 ids at a time, 55,186 bytes, about sixteen percent under a
pipe's capacity, and nothing in it says that was chosen. This record does not
depend on that being dangerous; it declines to depend on it being safe.


## Decision

### 1. One child per batch, told everything at once — not a handle

`host::process_bytes_input(program, args, input, timeout_ms)` runs a child
exactly as `host::process_bytes` does, and writes `input` to its standard
input, closing it afterwards. It returns what `process_bytes` returns: the two
streams as byte strings, the exit code, and the flags for a deadline, a
cancellation, and a truncated capture.

That is the whole interface. It reproduces the protocol above — write all the
requests, read all the replies, require exit 0 — and it turns four thousand
children into three.

`input` is a byte string, which costs a script nothing to produce: ids are
text, and `"…".as_bytes()` is already there. Taking bytes rather than text
means a child that wants bytes is served by the same function, and record
0016's rule holds — what a child is given is not decoded on the way in, as
what it says is not decoded on the way out.

### 2. All three streams move at once, and the writer can be abandoned

The host writes standard input on its own thread, beside the two capture
threads that already exist, so requests are being delivered while replies and
complaints are being drained. **What that removes is one class of hazard:
mutual pipe backpressure**, where a full reply pipe stops the child reading
and a full request pipe stops the parent writing. It is not a claim that
nothing can deadlock — a child that waits for something neither stream carries
still waits, and no arrangement of threads here changes that.

The script cannot arrange the removed hazard at all, because the script does
not do the writing: it hands over the whole input and receives the whole
output.

**The writer needs its own way out, and this is the part that is new rather
than inherited.** A descendant that leaves the process group can hold the read
end of standard input open without reading, even with the reply streams
redirected away, and then a large input blocks the writer after the direct
child is gone. So:

- **The writer gives up by itself.** The pipe is made non-blocking, and the
  writer retries with the growing wait record 0020 chose, checking the
  deadline and the interrupt flag between attempts. So it stops when the call
  stops, whoever is holding the read end.

  An earlier draft of this decision said the parent would close its end of
  standard input to make a blocked write fail. **That cannot be done**: the
  writer owns the pipe, and there is no other end for the parent to close.
  Making the write interruptible in the writer is the mechanism; the draft
  described one that does not exist.
- **The delivery gets a stop flag of its own, per call, and it is read before
  every attempt** rather than only when a write would block. A deadline is a
  time and the interrupt flag belongs to the whole process; neither of them
  says *this call is finished with you*, which is what has to be said when the
  direct child exits with input still undelivered.
- **The writer is always joined, and its outcome always collected — on every
  way out of the call.** An earlier draft joined it only if it had already
  finished and otherwise detached it, which discarded whatever it had to say,
  including the failure the line below promises to report, and left it holding
  its copy of the input. A later one still had two `?` on the wait itself, so
  a failure to wait for the child left by the front door with a thread still
  writing; the wait's result is now held and returned **after** the cleanup,
  which makes "always" true rather than nearly true. The join is bounded by
  construction: the pipe is non-blocking and the flag is read before every
  attempt, so the thread returns within one of its own waits.

- A write that fails for any reason other than the child closing the pipe is
  reported, after the process group is killed rather than before, so a failure
  never leaves a child behind.



This is deliberately not described as inheriting record 0020's limitation. That
one is about the capture joins; this is a third stream, in the other direction,
and it needs its own answer.

**Record 0020's limitation is still there, and it still bites.** The first
attempt at gate 3 used a descendant that left the process group holding *all
three* pipes, and the call hung for the descendant's whole life — on the
capture joins, exactly as record 0020 says, with the writer having already
given up. So the gate isolates the input stream by redirecting the reply
streams away. What this record fixes is the direction it adds; what it does
not fix is the one it inherited, and a gate that conflated them would have
credited this cut with the other's unfinished work.



**A child that stops reading is not an error.** `head -1` reads one line and
exits; the write then fails with a broken pipe, and that is the child's
prerogative rather than a fault. The call reports what the child said and the
status it exited with, exactly as if the input had all been consumed. A write
that fails for any other reason is reported.

### 3. What happens to the input, and what a success means

**The input is copied once, at the call boundary.** The bytes are taken from
the Rune value into an owned buffer the writer thread owns. The thread no
longer outlives the call — decision 2 sees to that — but it does outlive the
value the bytes came from, which the virtual machine may collect while the
writing is still going on. The copy is released when the writer
returns, which is before the call does.

**There is no size limit of its own.** A script that assembles a gigabyte of
input has already spent a gigabyte assembling it; what governs that is the
memory the script is allowed, not a second cap here. Nothing is streamed from
a file: the input is a value the script already holds.

**A success does not certify that every byte was consumed.** If the child
stops reading — decision 2's `head -1` — the write fails with a broken pipe,
that is not an error, and the call reports what the child said and the status
it exited with. What it does not report is how much of the input the child
took. A script that needs to know must ask the child, not the call: this
interface tells you what came back, not what got in.

### 4. Everything record 0016 and 0020 decided still holds

The deadline still ends the call and reports `timed_out`. The interrupt flag
still cancels. The process group is still killed. Each stream still carries
its own truncation flag and is capped at **two mebibytes** — the figure in
`host.rs`, not the eight an earlier draft of this record claimed, which is
`host::read`'s limit and not this one. A batch whose replies exceed two
mebibytes is reported as truncated rather than quietly cut. The reply stream
measured here is 534,882 bytes, about a quarter of the cap, so the workload
fits with room; a script that needs more must ask in more than one call.

The limitation record 0020 named is inherited unchanged: a descendant that
leaves the process group and holds a captured pipe open still delays the call
for as long as it lives.

### 5. What this record does not decide

**A long-lived child with interleaved requests and replies.** No workload
measured here needs one: this protocol is write-everything-then-read-
everything, and the verifier wants three children rather than one per commit.
A handle — open, write, read a line, read exactly *n* bytes, close, wait —
would put partial reads, per-call deadlines, cancellation with a request
outstanding, and what happens when the handle is dropped into the script's
hands, and none of that is needed to close this gap. It waits for a workload
that must read a reply before it knows the next request.

Whether the port should use this. Porting the verifier onto it is how gate 1
is measured, and record 0017's port is the thing being changed, so that is
this record's work — but the shape of the port is not a decision here beyond
matching the original's three batches.

## Acceptance gates

1. **The gap closes, and the answer does not change.** Measured, results
   before runtime, on the same generated 1,346-commit lineage:

   - **Standard output identical to the python original, byte for byte, and
     exit 0 from both.** The three-commit fixture's recorded diagnostics also
     still compare clean, so the rewrite is a change of shape and not of
     answers.
   - **Runtime, mean of three: 375 ms, from 4,949 ms.** Thirteen times, with
     python at 102 ms. The 4,038 invocations became three children; one batch
     of 4,038 objects on its own costs 37 ms through this function against
     4,500 ms as separate invocations.
   - **It costs more instructions, not fewer: between 34 and 36 million,
     where the per-object version needed 24.4 to 24.6.** The framing work
     moved from four thousand processes into Rune, and Rune is where the
     budget is counted. That is a real trade and it is recorded rather than
     buried: fewer children, more instructions, and an operator who must ask
     for them with record 0021's flag.

2. **All three streams under pressure at once.** A child is fed more than a
   pipe's worth of input — 165,558 bytes, the real request stream, and a
   mebibyte besides — **while writing more than a pipe's worth to standard
   output and to standard error simultaneously**, so every direction is
   backed up at the same time. The call returns with both reply streams whole
   (or flagged truncated at the cap, asserted either way) and the input
   delivered. One stream at a time would not exercise decision 2; three do.
3. **A deadline ends the call while input delivery is blocked.** A child that
   holds the read end of standard input open **through a preserved descriptor**
   and never reads it, fed more than a pipe's worth, is ended by its deadline: the call returns `timed_out`
   rather than waiting for a writer that cannot finish. Asserted for a
   descendant that has **left the process group**, which is the case the group
   kill cannot reach and which decision 2 answers by closing the parent's end
   and declining to join.
4. **A cancellation does the same.** The same child, interrupted rather than
   timed out, returns `cancelled` without waiting on the writer.

   Gates 3 to 6 run under a harness with **its own bound** — a deadline in
   the test, enforced by the test rather than by the thing under test — so a
   regression that hangs fails the suite instead of stopping it. That is the
   lesson record 0020 paid for: a timing assertion cannot be the guard, but a
   harness that cannot hang is necessary before a hang can be a failure.

5. **The delivery is gone before the call returns, not detached.** With a
   descendant that holds the read end of standard input outside the process
   group and a mebibyte still undelivered, the process has **one** thread once
   the call has returned, counted from `/proc` while that descendant is still
   alive. The call returning cannot distinguish a stopped writer from a
   detached one — both let it return — so this is observed from outside.
   Verified to fail against the detached draft, which showed two.

   **`exec 3<&0` is what makes these cases real.** A non-interactive shell
   gives a backgrounded command `/dev/null` for standard input, so
   `setsid sleep 5 &` does not hold the read end at all: the first version of
   these gates proved nothing, because what held the pipe was the foreground
   `sleep` inside the process group, and the group kill closed it — after
   which any writer stops on a broken pipe whether it reads a flag or not.
   Preserving the descriptor is what makes the descendant a holder, and the
   gates say so where they use it.

   **The handshakes are what make it a fact.** The descendant creates a file
   once it holds the descriptor, the direct child waits for that before
   exiting, and the descendant does not let go until the test writes another —
   so a test that has seen the first and not written the second knows the read
   end is held while it counts. Nothing here waits a fixed number of
   milliseconds and hopes, and the test's own receive of the call's "returned"
   line is bounded, because a blocking read would stop the suite instead of
   failing it and would do so before any assertion could run. Every failure
   path releases the descendant and closes the run's standard input before it
   judges.
6. **A failure arriving after cleanup begins is still reported.** It is
   **injected**, because it cannot be provoked: the errors a non-blocking pipe
   offers are a full buffer, an interruption, and a closed reader, and a
   closed reader is deliberately not a failure. So a `test-support` feature,
   off by default, lets the delivery fail at exactly the moment it observes
   the stop flag — the moment the call has begun cleaning up — and the gate
   asserts the message reaches the script as an error, that the call exits
   nonzero, and that it does not report success. Verified to fail against the
   detached draft, where the failure vanished with the thread.

   The gate lives in its own file, gated on the feature, and says so rather
   than skipping quietly: `cargo test --features test-support` runs it, and an
   ordinary build compiles a version of the hook that reads nothing.

7. **The harness bounds its own cleanup, and is tested against a run that
   never returns.** Every wait it does has a limit — the handshakes, the
   receive of a line, the reap, and the reading of the output, because a pipe
   a live descendant still holds never reaches the end of the file. The order
   is: release the descendant, **wait for it to acknowledge that it closed
   the descriptor**, reap the runner and kill it if it outstays its bound,
   read what it said, and only then remove the handshake files. Removing them
   first is what an earlier version did, and it left the descendant polling
   for a file that could never appear — a process stuck for as long as its own
   patience, which is why the descendant now also gives up by itself after
   about thirty seconds.

   **It also refuses to lose a failure.** Three signals went missing in
   earlier versions of it, each found by review and each now gated by the
   harness testing itself:

   - A read that timed out returned empty output, which an assertion about an
     empty stream would have been satisfied by. It returns a result now, and
     a stream that could not be read is trouble the test fails on rather than
     silence.
   - The acknowledgment was **not true**: `<&3` makes the descendant's
     standard input a duplicate and it inherits fd 3 as well, so closing
     standard input left the pipe held by a descriptor nobody was watching.
     Measured on `/proc/<pid>/fd`: two pipes held, one still held after
     `closed` under the old form, none under the new one, which closes both.
   - A run that outstayed its bound and was then killed reported the kill's
     success, erasing the hang. The bound being exceeded is remembered
     separately: cleanup working is not a reason to forget why it was needed.

   The harness is then pointed at a runner that cannot finish, and the test
   passes by **failing**: a bounded failure, the runner reaped, nothing left
   behind.
 That run spins in Rune rather than waiting on a long-lived child,
   because a child would sit in its own process group and killing the runner
   cannot clean up a group the runner never got to kill — a test of "nothing
   survives" must not itself create a survivor. Measured: a full suite run
   leaves no handshake directory and no helper process.
8. **A child that stops reading is reported as itself.** `head -1` fed a
   megabyte returns the one line it read and its exit status, with no error
   about a broken pipe and no hang.
9. **A child that says nothing and one that is given nothing both work.**
   Empty input, and a child that writes nothing, are ordinary cases rather
   than edge conditions that hang.
10. **The existing guarantees are unchanged.** A deadline, a cancellation, a
   killed process group, and per-stream truncation all behave for the new
   function as record 0016's and 0020's gates require of the old ones, tested
   through the new one rather than assumed from the old.
11. **The old functions are untouched.** `host::process` and
   `host::process_bytes` keep their signatures and behaviour, so the other
   three ports cannot notice this record happened.
12. **Nothing regresses.** The gates of records 0002 through 0021 pass, and all
   four ports still match their originals — the graft port through its
   rewritten form, which is the one whose comparison this record changes.

## Guardrails and stop conditions

1. No handle. If closing this gap seems to need interleaved reads, that is
   decision 5's record and not a widening of this one.
2. The input is written by the host, never by the script in pieces. A script
   that could write twice could deadlock, and the point of decision 2 is that
   it cannot.
3. The writer's outcome is never discarded. It is told to stop and then
   joined, every time; if that join ever needs a bound of its own, the reason
   is recorded rather than answered by detaching the thread again.
4. The three-batch shape follows the original. If the port needs a different
   division, the reason is recorded rather than chosen for convenience.

## Risks

- **A reply stream larger than two mebibytes is truncated.** The flag says so,
  and a script must then ask in more than one call. That limit predates this
  record and is not raised by it. This workload's replies are 534,882 bytes,
  but a batch four times this size would meet the cap, and the division into
  batches is the script's to get right.
- **The naive sequence may or may not hang.** The reproduction described in
  the Context completed rather than wedging, so the mutual-backpressure
  hazard is real in arithmetic and unobserved in this child. Decision 2 is
  worth having because it removes the hazard by construction and costs a
  thread that already has two siblings — not because the alternative was
  measured to fail.
- **The instruction cost went up.** A workload that was near the budget it
  was given will need more of it after this, and the operator is the one who
  has to notice. Gate 1 reports the new figure for the one workload measured;
  another script's is its own to find.
- **One child per batch is still a child per batch.** A workload with many
  small batches gains less than this one does. What it does not do is regress:
  a batch of one is a single child, which is what the port does per object
  today.
- **The port is being rewritten, and its comparison is the evidence.** If the
  rewrite drifts from the original's output, gate 1 fails and the rewrite is
  wrong; that is the intended order, results before runtime.

## Forward


The long-lived child of decision 4, if a workload asks for it. Bounding the
capture joins, which record 0020 left named and unfixed, and which this record
inherits. And a second script that reads bytes, still unclaimed: the
repository had no such need when it was last checked.
