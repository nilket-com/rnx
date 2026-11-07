import copy
import hashlib
from pathlib import Path
import tempfile
import unittest
from predicate_shape import normalize, check


class SnapshotControls(unittest.TestCase):
	def fixture(self, directory, assembly):
		path = Path(directory) / 'kernel.asm'
		path.write_text(assembly)
		name = 'rune::experimental_core::column::run::predicate::comparisons::<true, 4, &[f64], u64>'
		shape = dict(binary_sha256='test', functions=[dict(name=name, assembly=str(path), sha256=hashlib.sha256(path.read_bytes()).hexdigest())])
		snapshot = dict(functions={name: dict(instructions=normalize(assembly))})
		return shape, snapshot

	def test_exact_and_scalar_regression(self):
		with tempfile.TemporaryDirectory() as d:
			shape, snapshot = self.fixture(d, '10: cmpltpd %xmm0,%xmm1\n14: jne 10 <loop>')
			self.assertEqual(check(shape, snapshot)['status'], 'PASS')
			scalar, _ = self.fixture(d, '10: ucomisd %xmm0,%xmm1\n14: jne 10 <loop>')
			self.assertEqual(check(scalar, snapshot)['status'], 'FAIL')

	def test_missing_duplicate_and_digest(self):
		with tempfile.TemporaryDirectory() as d:
			shape, snapshot = self.fixture(d, '10: cmpltpd %xmm0,%xmm1')
			missing = copy.deepcopy(shape); missing['functions'] = []
			self.assertEqual(check(missing, snapshot)['status'], 'FAIL')
			duplicate = copy.deepcopy(shape); duplicate['functions'] *= 2
			with self.assertRaises(ValueError): check(duplicate, snapshot)
			Path(shape['functions'][0]['assembly']).write_text('changed')
			with self.assertRaises(ValueError): check(shape, snapshot)

	def test_relocation_preserves_local_branch(self):
		a = '10: mov 0x123(%rip),%rax # 100 <data>\n14: jne 10 <loop>'
		b = '20: mov 0x456(%rip),%rax # 200 <data>\n24: jne 20 <loop>'
		self.assertEqual(normalize(a), normalize(b))
		self.assertNotEqual(normalize(a), normalize(b.replace('jne 20', 'jne 24')))
