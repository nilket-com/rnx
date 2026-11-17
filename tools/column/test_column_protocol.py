"""Cheap controller controls run before builds, tests, or measurements."""
import importlib.util
from pathlib import Path
import unittest


def module(name):
	spec = importlib.util.spec_from_file_location("column_" + name, Path(__file__).with_name(name + ".py"))
	loaded = importlib.util.module_from_spec(spec)
	spec.loader.exec_module(loaded)
	return loaded


class Protocol(unittest.TestCase):
	def test_cli_rejects_wrong_routing_skips_counts_paths_and_caught_fuel(self):
		module("cli").controls()


if __name__ == "__main__":
	unittest.main()
