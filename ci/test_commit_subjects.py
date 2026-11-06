"""Signed temporary histories exercise policy without rewriting project history."""
from dataclasses import replace
from pathlib import Path
import json
import os
import subprocess
import sys
import tempfile
import unittest
from commit_subjects import evaluate, segment, Policy, HistoryError, IDENTITY


class History(unittest.TestCase):
	def setUp(self):
		self.temp = tempfile.TemporaryDirectory()
		self.addCleanup(self.temp.cleanup)
		self.root = Path(self.temp.name)
		self.repo = self.root / "repo"
		self.repo.mkdir()
		self.env = dict(os.environ)
		for key in list(self.env):
			if key.startswith("GIT_"):
				del self.env[key]
		self.env.update(GIT_CONFIG_NOSYSTEM="1", GIT_CONFIG_GLOBAL=os.devnull)
		key = self.root / "key"
		subprocess.run(["ssh-keygen", "-q", "-t", "ed25519", "-N", "", "-f", str(key)], check=True)
		allowed = self.root / "allowed"
		allowed.write_text(IDENTITY[1] + " " + key.with_suffix(".pub").read_text())
		self.git("init", "-q", "-b", "main")
		for name, value in [("user.name", IDENTITY[0]), ("user.email", IDENTITY[1]),
			("gpg.format", "ssh"), ("user.signingkey", str(key)), ("gpg.ssh.allowedSignersFile", str(allowed)), ("commit.gpgsign", "true")]:
			self.git("config", name, value)
		self.write("host", "host")
		self.base = self.commit("old host history")
		self.git("checkout", "--orphan", "engine")
		self.git("rm", "-rf", ".")
		self.write("engine", "engine")
		self.engine_base = self.commit("old engine history")
		self.plan("0206")
		self.write("engine", "new engine")
		self.engine_tip = self.commit("impl: adr 0206 engine")
		self.git("checkout", "main")
		self.plan("0208")
		parent = self.git("rev-parse", "HEAD")
		self.git("read-tree", "--prefix=rune/", "-u", self.engine_tip)
		tree = self.git("write-tree")
		self.import_commit = self.git("commit-tree", "-S", tree, "-p", parent, "-p", self.engine_tip, "-m", "impl: adr 0208 import history")
		self.git("reset", "--hard", self.import_commit)
		self.policy = Policy(host_base=self.base, engine_base=self.engine_base, engine_tip=self.engine_tip,
			import_commit=self.import_commit, follower_plan="", follower_impl="")

	def git(self, *args):
		p = subprocess.run(["git", *args], cwd=self.repo, env=self.env, text=True, capture_output=True)
		if p.returncode:
			raise AssertionError(p.stderr)
		return p.stdout.strip()

	def write(self, name, text):
		p = self.repo / name
		p.parent.mkdir(parents=True, exist_ok=True)
		p.write_text(text)

	def commit(self, message, *options):
		self.git("add", "-A")
		self.git("commit", "--allow-empty", *options, "-m", message)
		return self.git("rev-parse", "HEAD")

	def plan(self, number, status="Status: Accepted", extra=False):
		self.write(f"plans/adr/{number}_test.md", f"# Decision\n\n{status}\n")
		self.write(f"plans/impl/{number}_test.md", "# Implementation\n")
		if extra:
			self.write("unrelated", "bad")
		return self.commit(f"plan: adr {number} test")

	def check(self, policy=None):
		return evaluate(self.repo, policy or self.policy)

	def reject(self, text, policy=None):
		with self.assertRaisesRegex(HistoryError, text):
			self.check(policy)

	def test_valid_history_and_pending_plan(self):
		self.plan("0209")
		self.assertEqual(self.check()["status"], "PASS")
		self.write("tool", "implementation")
		self.commit("impl: adr 0209 checker")
		self.assertEqual(self.check()["status"], "PASS")

	def test_unsigned(self):
		self.plan("0209")
		self.commit("impl: adr 0209 unsigned", "--no-gpg-sign")
		self.reject("signature|git command failed")

	def test_identity(self):
		self.plan("0209")
		self.git("config", "user.name", "Wrong Person")
		self.commit("impl: adr 0209 wrong identity")
		self.reject("identity")

	def test_body(self):
		self.commit("plan: adr 0209 test\n\nbody")
		self.reject("body")

	def test_subject_forms(self):
		for subject in ["Plan checker", "plan: adr 209 short", "plan: adr 0209 Upper", "plan: adr 0209 scope(x)", "plan: adr 0209 cut a1", "plan: adr 0209 nonascii é"]:
			with self.subTest(subject=subject):
				self.commit(subject)
				self.reject("subject")
				self.git("reset", "--hard", self.import_commit)

	def test_implementation_before_plan(self):
		self.write("plans/adr/0209_test.md", "Status: Accepted\n")
		self.write("plans/impl/0209_test.md", "implementation")
		self.commit("impl: adr 0209 checker")
		self.reject("earlier first-parent plan")

	def test_duplicate_plan(self):
		self.plan("0209")
		self.plan("0209")
		self.reject("duplicate plan")

	def test_duplicate_implementation(self):
		self.plan("0209")
		self.commit("impl: adr 0209 first")
		self.commit("impl: adr 0209 second")
		self.reject("implementation limit")

	def test_plan_scope(self):
		self.plan("0209", extra=True)
		self.reject("exactly its paired")

	def test_missing_status(self):
		self.plan("0209", status="")
		self.reject("Status")

	def test_missing_implementation_document(self):
		self.write("plans/adr/0209_test.md", "Status: Accepted\n")
		self.commit("plan: adr 0209 checker")
		self.reject("impl document")

	def test_duplicate_shared_record(self):
		self.plan("0206")
		self.reject("already occupied")

	def test_other_record_edit(self):
		self.plan("0209")
		self.write("rune/plans/adr/0206_test.md", "Status: Accepted\nchanged")
		self.commit("impl: adr 0209 checker")
		self.reject("another record")

	def test_oversize(self):
		self.plan("0209", status="Status: Accepted\n" + "x\n" * 401)
		self.reject("400 lines")

	def test_cutoff_wrong_chain(self):
		self.reject("cutoff|ancestor|git command failed", replace(self.policy, host_base=self.engine_base))

	def test_import_tree_tamper(self):
		self.write("rune/engine", "tampered")
		self.git("add", "-A")
		tree = self.git("write-tree")
		parents = self.git("show", "-s", "--format=%P", self.import_commit).split()
		bad = self.git("commit-tree", "-S", tree, "-p", parents[0], "-p", parents[1], "-m", "impl: adr 0208 import history")
		self.git("reset", "--hard", bad)
		self.reject("subtree", replace(self.policy, import_commit=bad))


	def test_import_outside_prefix(self):
		self.write("host", "changed by import")
		self.git("add", "-A")
		tree = self.git("write-tree")
		parents = self.git("show", "-s", "--format=%P", self.import_commit).split()
		bad = self.git("commit-tree", "-S", tree, "-p", parents[0], "-p", parents[1], "-m", "impl: adr 0208 import history")
		self.git("reset", "--hard", bad)
		self.reject("outside prefix", replace(self.policy, import_commit=bad))

	def test_import_wrong_second_parent(self):
		self.reject("second parent", replace(self.policy, engine_tip=self.engine_base))

	def test_future_merge(self):
		self.plan("0209")
		parent = self.git("rev-parse", "HEAD")
		self.git("checkout", "-b", "side", self.import_commit)
		self.write("smuggled", "code")
		side = self.commit("unvalidated side commit")
		self.git("checkout", "main")
		tree = self.git("rev-parse", "HEAD^{tree}")
		merge = self.git("commit-tree", "-S", tree, "-p", parent, "-p", side, "-m", "impl: adr 0209 checker")
		self.git("reset", "--hard", merge)
		self.reject("unreviewed merge")

	def test_replace_refs(self):
		self.git("replace", self.base, self.engine_base)
		self.reject("replacement history")

	def test_grafts(self):
		self.write(".git/info/grafts", self.base + "\n")
		self.reject("grafted")

	def test_shallow(self):
		clone = self.root / "shallow"
		self.git("clone", "--depth=1", self.repo.as_uri(), str(clone))
		with self.assertRaisesRegex(HistoryError, "complete history"):
			evaluate(clone, self.policy)

	def test_follower_exact_pin(self):
		# A separate host before import has an impl-only follower plan.
		self.git("checkout", "--detach", self.base)
		self.write("plans/impl/0206_follower.md", self.policy.follower_reference)
		plan = self.commit("plan: adr 0206 follower")
		impl = self.commit("impl: adr 0206 follower")
		policy = replace(self.policy, follower_plan=plan, follower_impl=impl)
		self.assertEqual(len(segment(self.repo, self.base, impl, policy)["commits"]), 2)
		with self.assertRaisesRegex(HistoryError, "adr document"):
			segment(self.repo, self.base, impl, self.policy)


class Integration(unittest.TestCase):
	def test_host_preflight_is_first(self):
		root = Path(__file__).resolve().parents[1]
		sys.path.insert(0, str(root / "tools/column"))
		import accept
		self.assertEqual(accept.commands(root / "target/unused")[0],
			("commit-discipline", ["python3", "-B", "ci/preflight.py"]))

	def test_measurement_preflight_precedes_admission(self):
		import ast
		root = Path(__file__).resolve().parents[1]
		tree = ast.parse((root / "tools/column/ir_launch.py").read_text())
		calls = [node for node in ast.walk(tree) if isinstance(node, ast.Call)]
		preflight = next(node for node in calls if "ci/preflight.py" in ast.unparse(node) and isinstance(node.func, ast.Attribute) and node.func.attr == "run")
		admission = next(node for node in calls if isinstance(node.func, ast.Name) and node.func.id == "launch_budget")
		self.assertLess(preflight.lineno, admission.lineno)


if __name__ == "__main__":
	unittest.main()
