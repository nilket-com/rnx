"""Installed kernelspec opt-in survives restart; uses isolated Jupyter directories."""
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile


def main():
    kernel, worker = map(lambda s: str(Path(s).resolve()), sys.argv[1:3])
    with tempfile.TemporaryDirectory(prefix='rnx-jupyter-execution-') as temp:
        root = Path(temp)
        for key, name in [('JUPYTER_DATA_DIR', 'data'), ('JUPYTER_CONFIG_DIR', 'config'), ('JUPYTER_RUNTIME_DIR', 'runtime'), ('IPYTHONDIR', 'ipython')]:
            os.environ[key] = str(root/name)
        os.environ['PATH'] = str(Path(sys.executable).parent)+os.pathsep+os.environ['PATH']
        os.environ['RNX_CONFIG'] = ''
        from jupyter_client import KernelManager
        for option in ([], ['--engine=execution']):
            subprocess.run([kernel, 'install', '--rnx', worker]+option, check=True, capture_output=True)
        specs = root/'data/kernels'
        old = json.loads((specs/'rnx/kernel.json').read_text())
        expected = dict(argv=[kernel,'--connection-file','{connection_file}','--rnx',worker], display_name='Rune (rnx)', language='rune', interrupt_mode='message')
        assert old == expected, old
        new = json.loads((specs/'rnx-execution/kernel.json').read_text())
        assert new['argv'] == old['argv']+['--engine=execution']
        km = KernelManager(kernel_name='rnx-execution')
        try:
            km.start_kernel(env=dict(os.environ))
            client = km.client()
            client.start_channels()
            client.wait_for_ready(timeout=20)
            def execute(source):
                identity = client.execute(source)
                messages = []
                while True:
                    m = client.get_iopub_msg(timeout=20)
                    if m['parent_header'].get('msg_id') != identity:
                        continue
                    messages.append(m)
                    if m['msg_type']=='status' and m['content']['execution_state']=='idle':
                        break
                while True:
                    reply=client.get_shell_msg(timeout=20)
                    if reply['parent_header'].get('msg_id')==identity:
                        return reply['content'],messages
            reply, messages = execute('let keep=7;keep')
            assert reply['status']=='ok', reply
            assert any(m['msg_type']=='execute_result' and m['content']['data'].get('text/plain')=='7' for m in messages), messages
            client.stop_channels()
            km.restart_kernel(now=True)
            client=km.client();client.start_channels();client.wait_for_ready(timeout=20)
            reply,_=execute('async {7}.await')
            assert reply['status']=='error' and 'an async block is not available' in str(reply), reply
            reply,_=execute('keep')
            assert reply['status']=='error', reply
            reply,_=execute('9')
            assert reply['status']=='ok', reply
            client.stop_channels()
        finally:
            if km.has_kernel:
                km.shutdown_kernel(now=True)


if __name__=='__main__':
    main()
