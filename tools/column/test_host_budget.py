import os
import unittest
from unittest.mock import patch

import host_budget as budget

GIB = 1024 ** 3


def observation(free, headroom, cpus=28):
	return dict(cpu_ids=list(range(cpus)), mem_available_bytes=free, ancestor_headroom_bytes=headroom,
		binding_cgroup='/user.slice/final.service', binding_limit='memory.max', groups=[])


class HostBudget(unittest.TestCase):
	def test_an_idle_host_is_bounded_by_cpus(self):
		self.assertEqual(budget.build_jobs(observation(60*GIB, 60*GIB)), 19)
		self.assertEqual(budget.case_width(observation(60*GIB, 60*GIB)), 14)
		self.assertEqual(budget.build_jobs(observation(60*GIB, 60*GIB, cpus=4)), 4)

	def test_control_group_headroom_binds_below_host_free_memory(self):
		# A unit with 6 GiB headroom on a mostly idle 60 GiB host.
		tight = observation(60*GIB, 6*GIB)
		self.assertEqual(budget.build_jobs(tight), 1)
		self.assertEqual(budget.case_width(tight), 2)

	def test_too_little_memory_for_one_worker_is_refused(self):
		for size in (budget.build_jobs, budget.case_width):
			with self.subTest(size=size.__name__), self.assertRaisesRegex(ValueError, 'InsufficientMemory'):
				size(observation(60*GIB, 3*GIB))

	def test_a_single_cpu_still_runs_cases(self):
		self.assertEqual(budget.case_width(observation(60*GIB, 60*GIB, cpus=1)), 1)

	def test_inside_a_final_stages_use_its_budget(self):
		# Moved from test_stage_scheduler.py (ADR 0289): inside a final the
		# jobserver's width is the budget, measured once on a quiet host.
		with patch.dict(os.environ, {'MAKEFLAGS': '-j7 --jobserver-auth=fifo:/nonexistent'}):
			self.assertEqual(budget.build_jobs(), 7)
			self.assertEqual(budget.case_width(), 3)
		with patch.dict(os.environ, {'MAKEFLAGS': '-j7'}):
			self.assertIsNone(budget.inherited_width())


if __name__ == '__main__': unittest.main()
