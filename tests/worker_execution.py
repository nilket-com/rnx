"""0244 table: every row compares the real legacy and domain worker transport."""
import json
import os
from pathlib import Path
import signal
import select
import subprocess
import sys
import tempfile
from worker_parent import Parent

BINARY=sys.argv[1]
TABLE=[
    ('unit','Equal','let a=[1];let alias=a;let f=||7;'),
    ('alias','Equal','alias.push(2);a.len()+f()'),
    ('streams','Equal','print!("out");io::eprint("err")?;9'),
    ('compile','Wording','let broken = ;'),
    ('old-origin-definition','Equal','fn old() { panic!("old failure"); }'),
    ('old-origin','Wording','old()'),
    ('async-refusal','Policy','a.push(3);async {1}.await'),
]


def snapshot(reply,messages,streams):
    return dict(input=reply['input'],epoch=reply['epoch'],plain=reply['text_plain'],html=reply['text_html'],failure=reply['failure'],lost=reply['state_lost'],trace=[m['type'] for m in messages],streams={n:bytes(s.data) for n,s in streams.items()})


def run():
    workers=[Parent(BINARY,engine=e) for e in ('legacy','execution')]
    try:
        assert 'engine' not in workers[0].ready
        assert workers[1].ready['engine']=='execution'
        assert workers[1].ready['capabilities']=={'html':False,'async':False}
        for name,policy,source in TABLE:
            old,new=[snapshot(*w.execute(source)) for w in workers]
            assert old['trace']==new['trace']==['armed','settled'], (name,old,new)
            assert old['input']==new['input'] and old['epoch']==new['epoch'] and not old['lost'] and not new['lost']
            for result in (old,new):
                if result['failure']:
                    assert '<memory>' not in result['failure']['diagnostic'],(name,result)
            if policy=='Equal':
                assert old==new,(name,old,new)
            elif policy=='Wording':
                assert old['failure'] and new['failure'],(name,old,new)
                assert old['failure']['category']==new['failure']['category'],(name,old,new)
                assert old['failure']['origin']==new['failure']['origin'],(name,old,new)
                assert old['streams']==new['streams'],(name,old,new)
                if name=='old-origin':
                    frames='\n  at input 5, line 1, column 12\n  at input 6, line 1, column 1'
                    assert frames in new['failure']['diagnostic'],new
                    assert new['failure']['diagnostic'].replace(frames,'',1)==old['failure']['diagnostic'],(old,new)
            else:
                assert old['failure'] is None and old['plain']=='1',old
                assert 'async input is not available' in new['failure']['diagnostic'],new
                assert workers[0].execute('a.len()')[0]['text_plain']=='3'
                assert workers[1].execute('a.len()')[0]['text_plain']=='2'
            print(name,policy,'PASS')
        for w in workers:
            w.begin(op='reset');reply,_=w.settled();w.handoff()
            assert reply['epoch']==2 and reply['failure'] is None
        old,new=[snapshot(*w.execute('f()')) for w in workers]
        assert old['failure']['category']==new['failure']['category']=='compile'
        assert old['failure']['origin']==new['failure']['origin']
        for w in workers:
            w.begin('loop {}')
            assert w.message()['type']=='armed'
            w.p.send_signal(signal.SIGINT)
            reply,_=w.settled();w.handoff()
            assert reply['failure']['category']=='interrupted',reply
            assert w.execute('7')[0]['text_plain']=='7'
            w.begin(op='shutdown');reply,_=w.settled();w.handoff()
            assert not reply['state_lost'] and w.p.wait(timeout=5)==0
    finally:
        for w in workers:w.close()


def lost_armed():
    for engine in ('legacy','execution'):
        r,w=os.pipe();rr,ww=os.pipe()
        p=subprocess.Popen([BINARY,'worker','--control-read',str(r),'--control-write',str(ww),'--engine='+engine],pass_fds=(r,ww),stdout=subprocess.PIPE,stderr=subprocess.PIPE)
        os.close(r);os.close(ww)
        reader=os.fdopen(rr,'rb',buffering=0)
        try:
            assert json.loads(reader.readline())['type']=='ready'
            reader.close()
            os.write(w,json.dumps(dict(id=1,op='execute',source='println!("MUST-NOT-RUN");7',nonce='a'*64)).encode()+b'\n')
            out,err=p.communicate(timeout=10)
            assert p.returncode!=0 and b'MUST-NOT-RUN' not in out,(out,err)
        finally:
            os.close(w)
            if p.poll() is None:p.kill();p.wait()


def before_armed():
    with tempfile.TemporaryDirectory() as temp:
        marker=Path(temp)/'marker'; release=Path(temp)/'release'
        os.mkfifo(marker)
        fd=os.open(marker,os.O_RDONLY|os.O_NONBLOCK)
        w=Parent(BINARY,dict(os.environ,RNX_TEST_WORKER_PAUSE='armed',RNX_TEST_WORKER_PHASE_FILE=str(marker),RNX_TEST_WORKER_ARMED_RELEASE=str(release)),engine='execution')
        try:
            w.begin('println!("MUST-NOT-RUN");loop {}')
            assert select.select([fd],[],[],10)[0]
            assert os.read(fd,64)==b'armed'
            w.p.send_signal(signal.SIGINT)
            release.touch()
            reply,messages=w.settled();streams=w.handoff()
            assert [m['type'] for m in messages]==['armed','settled']
            assert reply['failure']['category']=='interrupted',reply
            assert b'MUST-NOT-RUN' not in bytes(streams['stdout'].data)
            # Drain the next rendezvous via a regular file, keeping release set.
            os.close(fd);fd=None;marker.unlink();marker.touch()
            assert w.execute('7')[0]['text_plain']=='7'
        finally:
            w.close()
            if fd is not None:os.close(fd)


if __name__=='__main__':
    run();lost_armed()
    if "--test-support" in sys.argv:before_armed()
    outcomes=[]
    for engine in ('legacy','execution'):
        w=Parent(BINARY,dict(os.environ,RNX_MEMORY_CEILING='1'),engine=engine)
        try:
            reply,messages,_=w.execute('42')
            outcomes.append((reply['failure']['category'],reply['input'],[m['type'] for m in messages]))
        finally:w.close()
    assert outcomes[0]==outcomes[1]==('over_ceiling',None,['settled']),outcomes
