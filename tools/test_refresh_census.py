import copy
import unittest
from refresh_census import digest, refresh, refresh_registry
import json


class RefreshControls(unittest.TestCase):
	def setUp(self):
		self.old = {'src/a.rs': b'old', 'src/b.rs': b'stable', 'src/lib.rs': b'fn core(){}\nend'}
		self.new = dict(self.old, **{'src/a.rs': b'new'})
		self.manifest = {'historical_registration': {'keep': 'historical'}, 'rows': [1, 2],
			'registration_policy': {'whole_files_sha256': {p: digest(self.old[p]) for p in ('src/a.rs', 'src/b.rs')},
			'install_core': {'path': 'src/lib.rs', 'start': 'fn core', 'end': '\nend', 'sha256': digest(b'fn core(){}')}}}

	def run_refresh(self, before=None, after=None, current=None, paths=None):
		return refresh(self.manifest, paths or ['src/a.rs'], (before or self.old).__getitem__,
			(after or self.new).__getitem__, (current or self.new).__getitem__)

	def test_only_named_live_pin_changes(self):
		original = copy.deepcopy(self.manifest)
		result = self.run_refresh()
		expected = copy.deepcopy(original)
		expected['registration_policy']['whole_files_sha256']['src/a.rs'] = digest(b'new')
		self.assertEqual(result, expected); self.assertEqual(self.manifest, original)

	def test_refuses_drift(self):
		for kwargs in [dict(before=self.new), dict(current=self.old),
			dict(after=dict(self.new, **{'src/b.rs': b'drift'}), current=dict(self.new, **{'src/b.rs': b'drift'})),
			dict(current=dict(self.new, **{'src/lib.rs': b'fn core(){changed}\nend'})),
			dict(paths=['src/missing.rs']), dict(paths=['src/a.rs', 'src/a.rs'])]:
			with self.subTest(kwargs=kwargs), self.assertRaises(ValueError): self.run_refresh(**kwargs)

	def test_registry_exact_delta_and_refusals(self):
		old = {'path': 'time::monotonic_ms', 'doc': 'old', 'hash': 'same'}
		new = dict(old, doc='new')
		added = {'path': 'time::monotonic_ns', 'doc': 'ns', 'hash': 'new'}
		a = {'registry': [old], 'lifecycle_closed': True, 'execution': 'not run; compile-only'}
		b = dict(a, registry=[new, added])
		policy = {'before_count': 1, 'after_count': 2, 'added': {added['path']: added},
			'changed': {old['path']: {'before': old, 'after': new}}}
		for name, w in [('before', a), ('after', b)]:
			policy[name + '_registry_sha256'] = digest(json.dumps(w['registry'],sort_keys=True,separators=(',', ':')).encode())
		manifest = dict(self.manifest, registry_rows=1)
		result = refresh_registry(manifest, a, b, policy)
		self.assertEqual(result, dict(manifest, registry_rows=2))
		for field in ['doc', 'hash', 'path']:
			bad = copy.deepcopy(b); bad['registry'][0][field] = 'unexpected'
			with self.assertRaises(ValueError): refresh_registry(manifest, a, bad, policy)
		bad = dict(b, lifecycle_closed=False)
		with self.assertRaises(ValueError): refresh_registry(manifest, a, bad, policy)
		bad = dict(b, registry=b['registry'] + [added])
		with self.assertRaises(ValueError): refresh_registry(manifest, a, bad, policy)


if __name__ == '__main__':
	unittest.main()
