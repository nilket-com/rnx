"""Bounded parent fixture. Bytes stay bytes; identities are fixed at collection."""

import json, os, queue, secrets, subprocess, threading, time

CAP = 2 * 1024 * 1024
WAIT = 5
LIVE = set()


class Stream:
    def __init__(self, marker, identity):
        self.marker, self.identity = marker, identity
        self.pending = b""
        self.data = bytearray()
        self.discarded = 0
        self.ended = False

    def keep(self, b):
        n = min(len(b), CAP - len(self.data))
        self.data.extend(b[:n])
        self.discarded += len(b) - n

    def feed(self, b):
        b = self.pending + b
        i = b.find(self.marker)
        if i >= 0:
            self.keep(b[:i])
            self.pending = b""
            self.ended = True
            return b[i + len(self.marker) :]
        n = max(0, len(b) - len(self.marker) + 1)
        self.keep(b[:n])
        self.pending = b[n:]
        return b""

    def eof(self):
        self.keep(self.pending)
        self.pending = b""


class Parent:
    def __init__(self, binary, env=None, probe=False):
        r, w = os.pipe()
        rr, ww = os.pipe()
        endpoints = [r, ww]
        kw = {}
        if os.name == "posix":
            kw["pass_fds"] = endpoints
        else:
            import msvcrt

            endpoints = [msvcrt.get_osfhandle(f) for f in endpoints]
            for h in endpoints:
                os.set_handle_inheritable(h, True)
            info = subprocess.STARTUPINFO()
            info.lpAttributeList = {"handle_list": endpoints}
            kw.update(
                startupinfo=info,
                close_fds=True,
                creationflags=subprocess.CREATE_NEW_PROCESS_GROUP,
            )
        try:
            command = (
                [binary, str(endpoints[0]), str(endpoints[1]), "isolated"]
                if probe
                else [
                    binary,
                    "--color=always",
                    "worker",
                    "--control-read",
                    str(endpoints[0]),
                    "--control-write",
                    str(endpoints[1]),
                ]
            )
            self.p = subprocess.Popen(
                command,
                stdin=subprocess.DEVNULL,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                env=env,
                **kw,
            )
        finally:
            os.close(r)
            os.close(ww)
        LIVE.add(self)
        self.write = os.fdopen(w, "wb", buffering=0)
        self.read = os.fdopen(rr, "rb", buffering=0)
        self.messages = queue.Queue(maxsize=16)
        self.lock = threading.Lock()
        self.streams = {}
        self.late = {n: bytearray() for n in ("stdout", "stderr")}
        self.eof = set()
        self.threads = []
        self.id = 0
        self.deadline = None
        self.expired = False

        def control():
            try:
                while True:
                    b = self.read.readline(262146)
                    if not b:
                        self.messages.put(None, timeout=WAIT)
                        break
                    if len(b) > 262145:
                        raise ValueError("oversized reply")
                    message = json.loads(b)
                    if message.get("type") == "settled":
                        self.settled_at = time.monotonic()
                        self.deadline = threading.Timer(
                            WAIT, self.expire, args=(message["id"],)
                        )
                        self.deadline.daemon = True
                        self.deadline.start()
                    self.messages.put(message, timeout=WAIT)
            except (OSError, ValueError) as e:
                self.messages.put(e, timeout=WAIT)

        self.spawn(control)
        for name, pipe in [("stdout", self.p.stdout), ("stderr", self.p.stderr)]:

            def collect(name=name, pipe=pipe):
                while True:
                    try:
                        b = os.read(pipe.fileno(), 8192)
                    except OSError:
                        b = b""
                    with self.lock:
                        s = self.streams.get(name)
                        if not b:
                            if s and not s.ended:
                                s.eof()
                            self.eof.add(name)
                            return
                        if s and not s.ended:
                            b = s.feed(b)
                        late = self.late[name]
                        late.extend(b[: max(0, CAP - len(late))])

            self.spawn(collect)
        self.ready = self.message(WAIT)
        assert self.ready["type"] == "ready", self.ready
        if not probe:
            assert self.ready.get("protocol") == 1, "unsupported worker protocol"

    def expire(self, request_id):
        # Independent of control reads, collection and a blocked output consumer.
        with self.lock:
            if self.id == request_id and self.streams:
                self.expired = True
                if self.p.poll() is None:
                    self.p.kill()

    def spawn(self, fn):
        t = threading.Thread(target=fn, daemon=True)
        t.start()
        self.threads.append(t)

    def message(self, timeout=None):
        value = self.messages.get(timeout=timeout)
        assert isinstance(value, dict), (
            "worker died or control failed",
            value,
            self.p.poll(),
        )
        return value

    def send(self, msg):
        self.write.write(json.dumps(msg).encode() + b"\n")

    def begin(self, source=None, op="execute"):
        self.id += 1
        self.expired = False
        nonce = secrets.token_hex(32)
        with self.lock:
            assert not self.streams, "previous output not handed off and acknowledged"
            self.streams = {
                n: Stream(
                    f"\x1eRNX-WORKER-1:{self.id}:{n}:{nonce}\x1f".encode(),
                    (self.p.pid, self.id, n),
                )
                for n in ("stdout", "stderr")
            }
        msg = dict(op=op, id=self.id, nonce=nonce)
        if source is not None:
            msg["source"] = source
        self.send(msg)
        return self.id

    def settled(self):
        messages = []
        while True:
            m = self.message()
            messages.append(m)
            assert m.get("id") == self.id, "reply belongs to a different operation"
            if m["type"] == "settled":
                break
        deadline = time.monotonic() + WAIT
        while True:
            with self.lock:
                done = all(s.ended for s in self.streams.values())
                dead = bool(self.eof) and not done
            assert not dead, "EOF before both barriers: no completion"
            if done:
                break
            assert time.monotonic() < deadline, "barrier deadline"
            time.sleep(0.001)
        return m, messages

    def handoff(self):
        if self.expired or time.monotonic() - self.settled_at > WAIT:
            self.close()
            raise TimeoutError("forwarding deadline expired; no ack")
        with self.lock:
            assert all(s.ended for s in self.streams.values())
            result = self.streams
            self.streams = {}
            if self.deadline:
                self.deadline.cancel()
        self.send(dict(op="ack", id=self.id))
        return result

    def execute(self, source):
        self.begin(source)
        reply, messages = self.settled()
        streams = self.handoff()
        return reply, messages, streams

    def close(self):
        LIVE.discard(self)
        if self.deadline:
            self.deadline.cancel()
        if self.p.poll() is None:
            self.p.kill()
        self.p.wait(timeout=WAIT)
        self.write.close()
        for t in self.threads:
            t.join(WAIT)
        self.read.close()
        self.p.stdout.close()
        self.p.stderr.close()
