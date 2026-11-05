"""ADR commit preflight. Historical commits before adoption are left untouched."""
import argparse
import json
from pathlib import Path, PurePosixPath
import re
import subprocess

BASE = "1b4b66bd3657868ecb1d49f8b3f355585b3ec98d"
SUBJECT = re.compile(r"(plan|impl): adr ([0-9]{4}) [a-z0-9][^\r\n]*")
PLAN_PATH = re.compile(r"plans/(adr|impl)/([0-9]{4})_[a-z0-9_]+\.md")


def validate(message, changed, tree):
	"""Validate committed metadata, not mutable working-copy substitutes."""
	match = SUBJECT.fullmatch(message.rstrip("\n"))
	if not match:
		raise ValueError("expected subject-only 'plan: adr XXXX ...' or 'impl: adr XXXX ...'")
	kind, number = match.groups()
	decisions = [name for name in tree if re.fullmatch(r"plans/adr/" + number + r"_[a-z0-9_]+\.md", name)]
	if len(decisions) != 1:
		raise ValueError(f"ADR {number} must have exactly one committed decision file")
	for name in changed:
		parts = PurePosixPath(name).parts
		if kind == "plan":
			path = PLAN_PATH.fullmatch(name)
			if not path or path[2] != number:
				raise ValueError(f"plan changes a non-plan or another ADR's path: {name}")
		elif parts[:2] in (("plans", "adr"), ("plans", "impl")):
			path = PLAN_PATH.fullmatch(name)
			if not path or path[2] != number:
				raise ValueError(f"implementation changes another ADR's plan: {name}")
	if kind == "plan" and not any(name.startswith(f"plans/impl/{number}_") for name in changed):
		raise ValueError("plan commit must include its implementation plan")
	return {"kind": kind, "adr": number, "decision": decisions[0]}


def validate_status(document):
	lines = document.splitlines()
	statuses = [line for line in lines if line.startswith("Status:")]
	if len(statuses) != 1 or not re.fullmatch(r"Status: [^\s].*", statuses[0]) or statuses[0] not in lines[:10]:
		raise ValueError("ADR requires one nonempty Status: line in its opening ten lines")


def git(repo, *args):
	return subprocess.check_output(["git", *args], cwd=repo, text=True)


def check(repo, base=BASE):
	head = git(repo, "rev-parse", "HEAD").strip()
	# No --base escape in the normal preflight: the committed adoption boundary
	# is fixed. The parameter exists for isolated temporary-repository controls.
	subprocess.run(["git", "merge-base", "--is-ancestor", base, head], cwd=repo, check=True)
	commits = git(repo, "rev-list", "--reverse", "--first-parent", base + ".." + head).splitlines()
	rows = []
	previous = base
	for commit in commits:
		parents = git(repo, "rev-list", "--parents", "-n", "1", commit).split()[1:]
		if not parents or parents[0] != previous:
			raise ValueError("adoption boundary is not on the complete first-parent chain")
		message = git(repo, "show", "-s", "--format=%B", commit)
		changed = git(repo, "diff-tree", "--no-commit-id", "--name-only", "-r", parents[0], commit).splitlines()
		tree = git(repo, "ls-tree", "-r", "--name-only", commit).splitlines()
		row = validate(message, changed, tree)
		validate_status(git(repo, "show", commit + ":" + row["decision"]))
		subprocess.run(["git", "verify-commit", commit], cwd=repo, check=True, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
		row["commit"] = commit
		rows.append(row)
		previous = commit
	return {"status": "PASS", "base": base, "head": head, "commits": rows}


def controls():
	adr = "plans/adr/0206_validity_aware_column_kernels.md"
	impl = "plans/impl/0206_validity_aware_column_kernels.md"
	validate("plan: adr 0206 column kernels\n", [adr, impl], [adr, impl])
	validate("impl: adr 0206 column kernels\n", ["crates/rune/src/lib.rs"], [adr, impl])
	bad = [
		("Plan CK2", [adr, impl], [adr, impl]),
		("plan: adr 206 short number", [adr, impl], [adr, impl]),
		("plan: adr 0206 Bad case", [adr, impl], [adr, impl]),
		("plan: adr 0206 valid\n\nSigned-off-by: someone", [adr, impl], [adr, impl]),
		("plan: adr 0206 valid", [adr, impl, "src/runtime.rs"], [adr, impl]),
		("plan: adr 0206 valid", [adr, impl, "plans/impl/0207_wrong.md"], [adr, impl]),
		("plan: adr 0206 valid", ["plans/old-plan.txt"], [adr, impl]),
		("plan: adr 0206 valid", [adr], [adr, impl]),
		("impl: adr 0206 valid", ["src/runtime.rs"], [impl]),
		("impl: adr 0206 valid", ["src/runtime.rs"], [adr, "plans/adr/0206_duplicate.md", impl]),
	]
	for args in bad:
		try:
			validate(*args)
		except ValueError:
			continue
		raise AssertionError(f"invalid record accepted: {args}")
	validate_status("# ADR 0206\n\nStatus: accepted 2026-11-04\n")
	missing_status = [
		"# ADR 0206\n",
		"# ADR 0206\nStatus:\n",
		"# ADR 0206\nStatus:    \n",
		"# ADR 0206\nStatus: accepted\nStatus: proposed\n",
		"\n" * 10 + "Status: accepted\n",
	]
	for document in missing_status:
		try:
			validate_status(document)
		except ValueError:
			continue
		raise AssertionError("missing/ambiguous ADR status accepted")
	return len(bad) + len(missing_status)


def main():
	parser = argparse.ArgumentParser(description=__doc__)
	parser.add_argument("--repo", type=Path, default=Path(__file__).resolve().parents[3])
	args = parser.parse_args()
	negative = controls()
	result = check(args.repo)
	result["negative_controls"] = negative
	print(json.dumps(result, indent=2))


if __name__ == "__main__":
	main()
