"""A build script that deletes its own rerun input reruns on every build.

Cargo then recompiles the package and everything above it, which made every
Cargo invocation in the final recompile rnx. This refuses that mechanism."""
from pathlib import Path
import re
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]
SHARED = ('tools/build_identity.rs',)


def scripts(root=ROOT):
	names = subprocess.check_output(['git', 'ls-files', '-z'], cwd=root).decode().split('\0')
	return [n for n in names if n and not n.startswith('probes/') and (n.endswith('/build.rs') or n == 'build.rs' or n in SHARED)]


def violations(text):
	# Unit tests compile only under `rustc --test`; a build never runs them.
	text = text.split('#[cfg(test)]')[0]
	return ['removes a file'] if re.search(r'remove_file\s*\(', text) else []


class BuildRerun(unittest.TestCase):
	def test_build_scripts_rerun_only_on_their_inputs(self):
		listed = scripts()
		self.assertIn('build.rs', listed)
		for name in listed:
			with self.subTest(script=name):
				self.assertEqual(violations((ROOT/name).read_text()), [])

	def test_the_always_rerun_marker_is_refused(self):
		marker = '''let marker = out.join("m");
	let _ = fs::remove_file(&marker);
	println!("cargo:rerun-if-changed={}", marker.display());'''
		self.assertIn('removes a file', violations(marker))
		self.assertIn('removes a file', violations(marker + '\n#[cfg(test)]\nmod tests {}'))

	def test_watched_paths_exist_in_every_reference_state(self):
		# Cargo treats a missing watched path as always out of date; the unit
		# tests build loose, packed and deleted-file checkouts.
		with tempfile.TemporaryDirectory() as tmp:
			binary = Path(tmp)/'identity-tests'
			subprocess.run(['rustc', '+1.98.1', '--edition', '2024', '--test', str(ROOT/'tools/build_identity.rs'),
				'-o', str(binary)], check=True, capture_output=True)
			done = subprocess.run([str(binary), '--test-threads=1'], capture_output=True, text=True)
			self.assertEqual(done.returncode, 0, done.stdout + done.stderr)
			self.assertIn('test result: ok. 4 passed', done.stdout)
