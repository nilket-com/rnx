"""Sibling names cannot change the plain-map capture boundary."""
import unittest
from unittest.mock import patch
from cli_symbols import symbols


class Symbols(unittest.TestCase):
	def test_reuse_siblings_are_excluded(self):
		raw = '0001 t map_symbol\n0002 t worker_symbol\n0003 t reuse_map\n0004 t reuse_worker\n'
		demangled = ('0001 t FunctionHandler::call_impl<rnx_polars::column::script::map, T>\n'
			'0002 t rnx_polars::column::apply_joined::<script::map::{closure#0}>\n'
			'0003 t FunctionHandler::call_impl<rnx_polars::column::script::map_reusing, T>\n'
			'0004 t rnx_polars::column::apply_joined_reusing::<script::map_reusing::{closure#0}>\n')
		with patch('cli_symbols.subprocess.check_output', side_effect=[raw, demangled]):
			self.assertEqual(symbols('fixture'), {'map': 'map_symbol', 'worker': 'worker_symbol'})
		with patch('cli_symbols.subprocess.check_output', side_effect=[raw, demangled.replace('script::map::{closure#0}', 'script::never_canceled')]):
			self.assertEqual(symbols('fixture')['worker'], 'worker_symbol')
		for extra in ['', '0005 t rnx_polars::column::apply_joined::<script::map::{closure#1}>\n']:
			bad = demangled.replace('0002 t rnx_polars::column::apply_joined::<script::map::{closure#0}>\n', '') if not extra else demangled + extra
			with patch('cli_symbols.subprocess.check_output', side_effect=[raw, bad]):
				with self.assertRaises(AssertionError):
					symbols('fixture')


if __name__ == '__main__':
	unittest.main()
