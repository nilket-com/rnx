"""Incomplete or mismatched documentation cannot count as a prepared suite."""
import copy
import unittest

from inventories import NIGHTLY, validate_inventory


class Inventories(unittest.TestCase):
	def test_rejects_incomplete_empty_and_wrong_provenance(self):
		pins = {'release': '0.55.2', 'cfg': 'adapter-json', 'toolchain': NIGHTLY,
			'crates': ['polars'], 'failed': [], 'resolved_polars_features': ['json', 'lazy']}
		inventory = {'provenance': {'release': '0.55.2', 'cfg': 'adapter-json', 'features': ['lazy', 'json']}, 'callables': [{'name': 'example'}]}
		validate_inventory('adapter-json', pins, inventory)
		for field, value in [('crates', []), ('failed', ['polars']), ('cfg', 'adapter'), ('toolchain', 'nightly')]:
			bad = copy.deepcopy(pins)
			bad[field] = value
			with self.subTest(field=field), self.assertRaises(AssertionError):
				validate_inventory('adapter-json', bad, inventory)
		for field, value in [('release', '0.54.4'), ('cfg', 'adapter'), ('features', ['lazy'])]:
			bad = copy.deepcopy(inventory)
			bad['provenance'][field] = value
			with self.subTest(field=field), self.assertRaises(AssertionError):
				validate_inventory('adapter-json', pins, bad)
		with self.assertRaises(AssertionError):
			validate_inventory('adapter-json', pins, dict(inventory, callables=[]))


if __name__ == '__main__':
	unittest.main()
