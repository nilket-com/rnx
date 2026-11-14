import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
import uuid
import importlib.util

from ir_resources import memory, fixed_budget, launch_budget, observed_budget

GIB = 1024**3


class Accounting(unittest.TestCase):
	def setUp(self):
		self.temp = tempfile.TemporaryDirectory(); self.addCleanup(self.temp.cleanup)
		self.base = Path(self.temp.name); self.root = self.base/'cg'; self.root.mkdir()
		self.meminfo = self.base/'meminfo'; self.meminfo.write_text(f'MemAvailable: {64*GIB//1024} kB\n')
		self.member = self.base/'membership'; self.member.write_text('0::/parent/child\n')
		self.group('parent'); self.group('parent/child')

	def group(self, name, *, limit='max', high='max', current=0, **fields):
		path = self.root/name; path.mkdir(parents=True, exist_ok=True)
		for key, value in [('memory.max', limit), ('memory.high', high), ('memory.current', current)]:
			(path/key).write_text(str(value)+'\n')
		row = dict(file=0, shmem=0, inactive_file=0, file_dirty=0, file_writeback=0, file_mapped=0)
		row.update(fields)
		(path/'memory.stat').write_text(''.join(f'{k} {v}\n' for k, v in row.items()))
		return path

	def observe(self, extra=None):
		return memory.observe(extra, root=self.root, meminfo=self.meminfo, membership=self.member, cpus=range(8))

	def test_cache_credit_and_anonymous_refusal_with_named_numbers(self):
		self.group('parent/child', limit=6*GIB, current=4*GIB, file=4*GIB, inactive_file=4*GIB)
		self.assertEqual(fixed_budget(self.observe()), 2)
		self.group('parent/child', limit=6*GIB, current=5*GIB)
		for budget in (fixed_budget, lambda r: observed_budget(r, 2*GIB, 2*GIB)):
			with self.assertRaisesRegex(ValueError, 'InsufficientMemory.*binding_cgroup=/parent/child.*reserve_bytes=2147483648.*workers=0') as caught:
				budget(self.observe())
			self.assertEqual(caught.exception.numbers['host_available_bytes'], 64*GIB)
			self.assertEqual(caught.exception.numbers['cgroup_headroom_bytes'], GIB)

	def test_high_ancestor_destination_host_and_unlimited(self):
		self.assertEqual(self.observe()['binding_cgroup'], 'host')
		self.group('parent', limit=8*GIB, high=5*GIB, current=GIB)
		row = self.observe(); self.assertEqual(row['ancestor_headroom_bytes'], 4*GIB)
		self.assertEqual((row['binding_cgroup'], row['binding_limit']), ('/parent', 'memory.high'))
		self.group('destination', limit=3*GIB)
		row = self.observe('/destination'); self.assertEqual(row['ancestor_headroom_bytes'], 3*GIB)
		self.assertEqual(row['binding_cgroup'], '/destination')
		self.assertEqual(len(row['groups']), 3)
		self.meminfo.write_text('MemAvailable: 1024 kB\n')
		self.assertEqual(self.observe()['binding_cgroup'], 'host')

	def test_no_active_shared_dirty_writeback_or_mapped_credit(self):
		self.group('parent/child', limit=1000, current=900, file=800, shmem=100, inactive_file=600,
			file_dirty=100, file_writeback=50, file_mapped=200)
		row = self.observe()['groups'][0]
		self.assertEqual(row['cache_credit_bytes'], 250)
		self.assertEqual(row['limits']['memory.max']['usable_bytes'], 350)
		self.group('parent/child', limit=1000, current=900, file=800, inactive_file=0)
		self.assertEqual(self.observe()['groups'][0]['cache_credit_bytes'], 0)

	def test_missing_malformed_and_linked_fail_closed(self):
		for name in ('memory.stat', 'memory.current', 'memory.max', 'memory.high'):
			path = self.group('parent/child')/name; path.unlink()
			with self.assertRaises(memory.ResourceObservationError): self.observe()
			self.group('parent/child')
		for name, text in [('memory.current','-1'), ('memory.max','oops'), ('memory.high','1.5'), ('memory.stat','file 1\n')]:
			(self.group('parent/child')/name).write_text(text)
			with self.assertRaises(memory.ResourceObservationError): self.observe()
			self.group('parent/child')
		with self.assertRaises(memory.ResourceObservationError): self.observe('/../escape')
		(self.root/'linked').symlink_to(self.root/'parent', target_is_directory=True)
		with self.assertRaises(memory.ResourceObservationError): self.observe('/linked')

	def test_zero_observations_refuse_and_overrides_do_not_force(self):
		with self.assertRaises(memory.InsufficientMemory): launch_budget([0], 0, 0, 1, 0)
		self.assertEqual(memory.capacity(8, 100, 100, 10, 20, requested=99), (4, 90))
		with self.assertRaises(ValueError): memory.capacity(8,100,100,10,20,requested=0)
		with self.assertRaises(memory.ResourceObservationError): memory.capacity(8,100,-1,10,20)
		path = Path(__file__).resolve().parents[2]/'rune/tools/xtask/supplemental/admission.py'
		# Supplemental admission imports the same implementation under its local module name.
		from unittest.mock import patch
		with patch.dict(sys.modules, {'memory_resources': memory}):
			spec = importlib.util.spec_from_file_location('memory_admission_control', path)
			admission = importlib.util.module_from_spec(spec); spec.loader.exec_module(admission)
		with self.assertRaises(memory.InsufficientMemory) as caught: admission.admit(8,64*GIB,0,4*GIB)
		self.assertEqual(caught.exception.numbers['per_worker_bytes'], 5*GIB)

	def test_live_cache_and_anonymous_with_high_binding(self):
		if not Path(f'/run/user/{os.getuid()}/bus').exists(): self.skipTest('user systemd bus unavailable; run memory_live.py in a limited unit')
		# /tmp may be tmpfs: swap-backed shmem must not earn file-cache credit.
		target = Path(__file__).resolve().parents[2]/'target'; target.mkdir(exist_ok=True)
		live = tempfile.TemporaryDirectory(prefix='memory-control-', dir=target); self.addCleanup(live.cleanup)
		out = Path(live.name)/'live'
		command = ['systemd-run','--user','--wait','--collect','--quiet',
			'--unit=rnx-memory-control-'+uuid.uuid4().hex,
			'--property=MemoryMax=6G','--property=MemoryHigh=5G','--property=MemorySwapMax=0',
			'--property=RuntimeMaxSec=180',sys.executable,'-B',str(Path(__file__).with_name('memory_live.py').resolve()),'--out',str(out)]
		done = subprocess.run(command,capture_output=True,text=True,timeout=240)
		self.assertEqual(done.returncode,0,done.stderr + ((out/'RESULT.json').read_text() if (out/'RESULT.json').exists() else ''))
		row = json.loads((out/'RESULT.json').read_text()); self.assertEqual(row['status'],'PASS')
		self.assertEqual(row['observations'][1]['workers'],1)
		self.assertEqual(row['observations'][2]['refusal']['workers'],0)
		self.assertEqual(row['release']['status'],'PASS'); self.assertFalse((out/'payload').exists())


if __name__ == '__main__': unittest.main()
