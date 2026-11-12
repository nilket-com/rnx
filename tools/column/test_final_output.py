"""Exercise the Rust output-assignment boundary in isolated processes."""
import os
from pathlib import Path
import subprocess
import tempfile
import unittest


class RoutingAssignment(unittest.TestCase):
	def test_final_requires_assigned_root(self):
		root=Path(__file__).resolve().parents[2]
		with tempfile.TemporaryDirectory() as tmp:
			p=Path(tmp);source=p/'probe.rs';binary=p/'probe'
			source.write_text('#[path='+repr(str(root/'tests/support/final_output.rs')).replace("'",'"')+'] mod output;\nfn main(){println!("{:?}",output::assigned("RNX_FINAL_ROUTING_ROOT","cli"));}\n')
			subprocess.run(['rustc','+1.98.1','--edition=2024',str(source),'-o',str(binary)],check=True,capture_output=True)
			env={k:v for k,v in os.environ.items() if k not in {'RNX_FINAL','RNX_FINAL_ROUTING_ROOT'}}
			self.assertEqual(subprocess.run([binary],env=env,capture_output=True).returncode,0)
			env['RNX_FINAL']='1'
			failed=subprocess.run([binary],env=env,capture_output=True)
			self.assertNotEqual(failed.returncode,0);self.assertIn(b'not assigned',failed.stderr)
			env['RNX_FINAL_ROUTING_ROOT']=str(p)
			self.assertEqual(subprocess.run([binary],env=env,capture_output=True).returncode,0)
			link=p/'link';link.symlink_to(p,target_is_directory=True);env['RNX_FINAL_ROUTING_ROOT']=str(link)
			self.assertNotEqual(subprocess.run([binary],env=env,capture_output=True).returncode,0)
