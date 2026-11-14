#!/usr/bin/env python3
"""PTY dependency handover controls; helper speaks the real bounded protocol."""
import json
import os
import pathlib
import pty
import select
import signal
import socket
import struct
import subprocess
import sys
import tempfile
import time


def frame(kind, fields):
    body = bytes([1, kind])
    for tag, value in sorted(fields.items()):
        value = value.encode()
        body += bytes([tag]) + struct.pack('!I', len(value)) + value
    return struct.pack('!I', len(body)) + body


def receive(sock):
    def exact(n):
        data = b''
        while len(data) < n:
            part = sock.recv(n - len(data))
            assert part, 'truncated request'
            data += part
        return data
    body = exact(struct.unpack('!I', exact(4))[0])
    assert body[0] == 1
    fields, at = {}, 2
    while at < len(body):
        tag, size = body[at], struct.unpack('!I', body[at+1:at+5])[0]
        fields[tag] = body[at+5:at+5+size].decode()
        at += 5 + size
    return body[1], fields


def helper():
    sock = socket.socket(fileno=int(os.environ['RNX_INTERNAL_DEP_FD']))
    kind, request = receive(sock)
    if kind == 6:
        sock.sendall(frame(6, {1: '1'}))
        return
    assert kind == 1
    root = pathlib.Path(os.environ['DEP_FIXTURE_ROOT'])
    (root / 'request.json').write_text(json.dumps(request))
    engine, mode = request[8], os.environ['DEP_FIXTURE_MODE']
    assert engine in ('legacy', 'execution')
    if mode == 'failure':
        sock.sendall(frame(8, {1: 'fixture preparation refused'}))
        return
    choice = {'unknown': 'future', 'mismatch': 'execution' if engine == 'legacy' else 'legacy'}.get(mode, engine)
    description = {1: 'candidate', 2: 'fixture description', 3: '', 4: '', 5: '' if mode == 'installed' else 'polars', 6: '', 7: '', 8: choice}
    sock.sendall(frame(2, description))
    if mode in ('unknown', 'mismatch'):
        sock.recv(1)
        return
    kind, consent = receive(sock)
    if mode in ('installed', 'decline'):
        assert kind == 3 and not consent
        return
    assert kind == 4 and consent == {1: 'candidate', 2: engine}
    if mode == 'cancel':
        print('PREPARATION-WAIT', flush=True)
        signal.pause()
        return
    path = root / 'replacement'
    stat = path.stat()
    stamp = f'{stat.st_size}:{stat.st_mtime_ns // 1000000000}:{stat.st_mtime_ns % 1000000000}:true:{stat.st_dev}:{stat.st_ino}'
    association = frame(7, {1: '', 2: str(root/'manager'), 3: '', 4: '', 5: '', 6: str(path)}).hex()
    sock.sendall(frame(5, {1: str(path), 2: association, 3: stamp, 8: engine if mode != 'ready-mismatch' else ('execution' if engine == 'legacy' else 'legacy')}))


def run(binary, engine, mode, scratch=False):
    with tempfile.TemporaryDirectory(prefix='rnx-dep-engine-') as directory:
        root = pathlib.Path(directory)
        manager = root / 'manager'
        manager.write_text('#!/bin/sh\nexec python3 ' + __import__('shlex').quote(str(pathlib.Path(__file__).resolve())) + ' --helper\n')
        manager.chmod(0o755)
        replacement = root / 'replacement'
        replacement.write_text('#!/usr/bin/env python3\nimport os,sys,json\nfrom pathlib import Path\nPath(os.environ["DEP_FIXTURE_ROOT"],"argv.json").write_text(json.dumps(sys.argv[1:]))\nos.environ.pop("RNX_INTERNAL_SESSION_V1",None)\nos.execv(os.environ["DEP_FIXTURE_BINARY"],[os.environ["DEP_FIXTURE_BINARY"]]+sys.argv[1:])\n')
        replacement.chmod(0o755)
        association = frame(7, {1: '', 2: str(manager), 3: '', 4: '', 5: '', 6: binary}).hex()
        master, slave = pty.openpty()
        env = dict(os.environ, RNX_CONFIG='', RNX_HISTORY='/dev/null', NO_COLOR='1', TERM='dumb', RNX_INTERNAL_SESSION_V1=association, DEP_FIXTURE_ROOT=directory, DEP_FIXTURE_BINARY=binary, DEP_FIXTURE_MODE=mode)
        if scratch:
            env['RNX_INTERNAL_SESSION_V1'] = ''
            env['RNX_PROJECT_TOOL'] = str(manager)
        marker_fd = None
        if mode in ('recheck', 'exec'):
            marker = root / 'marker'
            os.mkfifo(marker)
            marker_fd = os.open(marker, os.O_RDONLY | os.O_NONBLOCK)
            env.update(RNX_DEP_PAUSE='after-cleanup' if mode == 'recheck' else 'before-exec', RNX_DEP_MARKER=str(marker), RNX_DEP_RELEASE=str(root/'release'), RNX_DEP_TIMELINE=str(root/'timeline'))
        child = subprocess.Popen([binary, '--no-splash', 'repl', '--engine='+engine], stdin=slave, stdout=slave, stderr=slave, env=env, start_new_session=True)
        os.close(slave)
        pending = b''
        transcript = b''
        def until(needle):
            nonlocal pending, transcript
            deadline = time.monotonic()+20
            while needle not in pending:
                remaining = deadline-time.monotonic()
                assert remaining > 0 and select.select([master], [], [], remaining)[0], (mode, engine, transcript)
                data = os.read(master, 65536)
                assert data, transcript
                pending += data
                transcript += data
            before, pending = pending.split(needle, 1)
            return before
        def send(line):
            os.write(master, line.encode()+b'\n')
        try:
            until(b'> ')
            send('let a=[1];let alias=a;let c=||7;0')
            until(b'> ')
            send(':depv polars' if mode == 'decline' else ':dep polars')
            if mode == 'decline':
                until(b'Continue? [y/N] ')
                send('n')
            elif mode == 'cancel':
                until(b'PREPARATION-WAIT')
                child.send_signal(signal.SIGINT)
            if mode in ('recheck', 'exec'):
                assert select.select([marker_fd], [], [], 20)[0], 'missing cleanup boundary'
                assert os.read(marker_fd, 65536).decode() == str(replacement)
                if mode == 'recheck':
                    replacement.write_text(replacement.read_text()+'# changed\n')
                else:
                    replacement.chmod(0o644)
                (root/'release').touch()
                until(b'dependency restart')
                assert child.wait(timeout=15) != 0
                assert not (root/'argv.json').exists()
                trace = (root/'timeline').read_text()
                assert trace.count('cleanup-begin') == 1, trace
                assert trace.count('after-cleanup') == 1, trace
                return
            until(b'> ')
            if mode == 'success':
                actual = json.loads((root/'argv.json').read_text())
                assert actual == ['--no-splash', 'repl', '--engine='+engine], actual
                send('a.len()')
                output = until(b'> ')
                assert b'error' in output.lower(), output
                send(':reset')
                until(b'> ')
                send('async {7}.await')
                output = until(b'> ')
                assert (b'an async block is not available' in output) == (engine == 'execution'), output
                if engine == 'execution':
                    assert b'start `rnx repl` without --engine=execution' in output, output
                    assert b'select the `' not in output and b'use the legacy session engine' not in output, output
            else:
                assert not (root/'argv.json').exists()
                send('alias.push(2);a.len()+c()')
                output = until(b'> ')
                assert b'9\r\n' in output, output
            send(':quit')
            assert child.wait(timeout=15) == 0, transcript
        finally:
            if child.poll() is None:
                child.kill()
                child.wait()
            os.close(master)
            if marker_fd is not None:
                os.close(marker_fd)


def startup(binary):
    for engine in (None, 'legacy', 'execution', 'unknown'):
        parent, child = socket.socketpair()
        args = [binary, '--no-splash', 'repl']
        if engine is not None:
            args.append('--engine='+engine)
        process = subprocess.Popen(args, env=dict(os.environ, RNX_CONFIG='', RNX_INTERNAL_STARTUP_FD=str(child.fileno())), pass_fds=(child.fileno(),), stdout=subprocess.PIPE, stderr=subprocess.PIPE)
        child.close()
        parent.settimeout(15)
        try:
            if engine == 'unknown':
                out, err = process.communicate(timeout=15)
                assert process.returncode != 0 and b'invalid session engine' in err, (out, err)
                assert parent.recv(1) == b''
            else:
                kind, fields = receive(parent)
                expected = {1: '42'}
                if engine is not None:
                    expected[2] = engine
                assert kind == 6 and fields == expected, fields
                out, err = process.communicate(timeout=15)
                assert process.returncode == 0, (out, err)
        finally:
            parent.close()
            if process.poll() is None:
                process.kill()
                process.wait()


if __name__ == '__main__':
    if sys.argv[1] == '--helper':
        helper()
    else:
        startup(sys.argv[1])
        for engine in ('legacy', 'execution'):
            for mode in ('success', 'installed', 'failure', 'decline', 'cancel', 'unknown', 'mismatch', 'ready-mismatch', 'recheck', 'exec'):
                run(sys.argv[1], engine, mode, scratch=len(sys.argv) > 2 and sys.argv[2] == "scratch")
