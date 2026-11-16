import unittest

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

	def test_a_final_holds_a_quarter_of_its_memory_back(self):
		# A unit with 48 GiB headroom: 15 jobs fill it; the final keeps 12 GiB for
		# page cache, test children and controllers, so 12 jobs.
		unit = observation(60*GIB, 48*GIB)
		self.assertEqual(budget.build_jobs(unit), 15)
		self.assertEqual(budget.final_jobs(unit), 12)
		self.assertEqual(budget.final_jobs(observation(60*GIB, 60*GIB, cpus=4)), 4)
		with self.assertRaisesRegex(ValueError, 'InsufficientMemory'):
			budget.final_jobs(observation(60*GIB, 4*GIB))

	def test_a_single_cpu_still_runs_cases(self):
		self.assertEqual(budget.case_width(observation(60*GIB, 60*GIB, cpus=1)), 1)


if __name__ == '__main__': unittest.main()
