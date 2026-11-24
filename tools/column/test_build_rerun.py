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

class ArchiveIdentity(unittest.TestCase):
	def test_export_binding_and_warm_invalidation(self):
		import os
		import shutil
		with tempfile.TemporaryDirectory() as temp:
			parent = Path(temp)
			repo = parent/'producer'
			export = parent/'consumer'
			repo.mkdir()
			export.mkdir()
			for rel in ('.gitattributes', 'tools/export-source.sh', 'tools/build_identity.rs'):
				dst = repo/rel
				dst.parent.mkdir(exist_ok=True)
				shutil.copyfile(ROOT/rel, dst)
			(repo/'rune').mkdir()
			(repo/'src').mkdir()
			(repo/'Cargo.toml').write_text('[package]\nname="identity-probe"\nversion="0.0.0"\nedition="2024"\n')
			(repo/'build.rs').write_text('#[path="tools/build_identity.rs"] mod identity; fn main(){identity::emit(std::path::Path::new("."));}')
			(repo/'src/main.rs').write_text('fn main(){println!("{}",env!("RNX_ENGINE_IDENTITY"));}')
			def git(root, *args):
				return subprocess.check_output(['git', '-C', str(root), *args], text=True).strip()
			for root in (parent, repo):
				git(root, 'init', '-q')
				git(root, 'config', 'user.name', 'fixture')
				git(root, 'config', 'user.email', 'fixture@invalid')
				git(root, 'config', 'commit.gpgsign', 'false')
			git(parent, 'commit', '--allow-empty', '-qm', 'enclosing repository')
			env = dict(os.environ, CARGO_TARGET_DIR=str(parent/'warm-target'))
			def invoke():
				return subprocess.run(['cargo', '+1.98.1', 'run', '-q', '--offline'], cwd=export,
					env=env, capture_output=True, text=True)
			for value in ('first', 'second'):
				(repo/'rune/data').write_text(value)
				git(repo, 'add', '-A')
				git(repo, 'commit', '-qm', value)
				rev, tree = git(repo, 'rev-parse', 'HEAD'), git(repo, 'rev-parse', 'HEAD:rune')
				archive = subprocess.check_output(['sh', 'tools/export-source.sh', rev], cwd=repo)
				# Restore current mtimes after extraction, as when changing a
				# declaration in a live checkout with an existing Cargo target.
				subprocess.run(['tar', '-xm', '-C', str(export)], input=archive, check=True)
				proof = export/'.rnx-export'
				saved = proof.read_bytes()
				if value == 'second':
					proof.write_bytes(previous)
					result = invoke()
					self.assertNotEqual(result.returncode, 0)
					self.assertIn('export revision', result.stderr)
				proof.write_bytes(saved)
				result = invoke()
				self.assertEqual(result.returncode, 0, result.stderr)
				self.assertEqual(result.stdout.strip(), f'0.15.0 (monorepo {rev}; engine {tree}; dirty=false)')
				previous = saved
			proof.write_bytes(saved.replace(tree.encode(), b'0'*40, 1))
			result = invoke()
			self.assertNotEqual(result.returncode, 0)
			self.assertIn('rune subtree differs', result.stderr)
			proof.unlink()
			result = invoke()
			self.assertNotEqual(result.returncode, 0)
			self.assertIn('missing export identity', result.stderr)
