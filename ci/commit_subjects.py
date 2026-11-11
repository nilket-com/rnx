#!/usr/bin/env python3
"""Read-only committed metadata and ADR lineage preflight (adapted from ket)."""
import argparse
from dataclasses import dataclass
import json
from pathlib import Path
import re
import subprocess

IDENTITY = ("Joel Bondurant", "em@joelbondurant.com")
MAX_IMPLEMENTATIONS = 1
SUBJECT = re.compile(r"^(plan|impl): adr ([0-9]{4}) ([a-z0-9][\x20-\x7e]*)$")
LABELS = re.compile(r"\b(?:cut [a-z][0-9]*|e[0-9]+(?:[._-][0-9]+)*|(?:crypto|pd|f-own|f-ctr)[-_][0-9]+)\b", re.I)
DOC = re.compile(r"(?:rune/)?plans/(adr|impl)/([0-9]{4})[_-][a-z0-9_-]+\.md$")


class HistoryError(ValueError):
	def __init__(self, message, *, commit=None, rule=None):
		super().__init__(message)
		self.commit = commit
		self.rule = rule


@dataclass(frozen=True)
class Policy:
	sequence_base: str = "bce96e3b3d7ec3ee4c444de9bb4e7976e7e59ddc"
	host_base: str = "2b6b76b3692fff98afb247c76047f99cb1c8d51f"
	engine_base: str = "1b4b66bd3657868ecb1d49f8b3f355585b3ec98d"
	engine_tip: str = "6d7aa59dee00dd9c5c598b3a1e0d54fca6ae6bdf"
	import_commit: str = "3930b1cb454198235edb1124b37fe46aa2cbcfed"
	follower_plan: str = "b82497a1897b62f2c9277b25c2bd1d629194e959"
	follower_impl: str = "2c47218d0de2f8cf93f2c6961bf677ea71cdc5be"
	follower_reference: str = "https://github.com/nilket-com/rune/blob/fed878b5aea151a878462d71626c9a4d589a4273/plans/adr/0206_validity_aware_column_kernels.md"


def git(repo, *args):
	p = subprocess.run(["git", "--no-replace-objects", "-C", str(repo), *args],
		capture_output=True, text=True, encoding="utf-8", errors="strict", timeout=60)
	if p.returncode:
		raise HistoryError(p.stderr.strip() or "git command failed")
	return p.stdout


def need(condition, message):
	if not condition:
		raise HistoryError(message)


def metadata(repo, commit):
	fields = git(repo, "show", "-s", "--format=%H%x00%P%x00%an%x00%ae%x00%cn%x00%ce", commit).rstrip("\n").split("\0")
	need(len(fields) == 6 and fields[0] == commit, "malformed metadata")
	message = git(repo, "show", "-s", "--format=%B", commit)
	need(tuple(fields[2:4]) == IDENTITY and tuple(fields[4:6]) == IDENTITY, "author/committer identity mismatch")
	need(not message.partition("\n")[2].strip(), "commit body forbidden")
	subject = message.split("\n", 1)[0]
	match = SUBJECT.fullmatch(subject)
	need(match is not None and not any(c in subject for c in "()") and not LABELS.search(subject), "invalid subject")
	kind, number, _ = match.groups()
	need(int(number) >= 206, "record number predates adoption")
	git(repo, "verify-commit", commit)
	return {"commit": commit, "parents": fields[1].split(), "subject": subject, "kind": kind, "number": number}


def paths(repo, commit):
	return git(repo, "ls-tree", "-r", "--name-only", commit).splitlines()


def status(text):
	lines = text.splitlines()
	found = [line for line in lines if line.startswith("Status:")]
	need(len(found) == 1 and bool(re.fullmatch(r"Status: \S.*", found[0])) and found[0] in lines[:10], "missing/ambiguous opening Status")
	need(len(lines) <= 400, "decision exceeds 400 lines; separate reviewed policy required")


def document(repo, commit, tree, number, category, prefix=""):
	pattern = re.compile(re.escape(prefix) + r"plans/" + category + "/" + number + r"[_-][a-z0-9_-]+\.md$")
	found = [p for p in tree if pattern.fullmatch(p)]
	need(len(found) == 1, "record must have exactly one " + category + " document: " + number)
	if category == "adr":
		status(git(repo, "show", commit + ":" + found[0]))
	return found[0]


def import_proof(repo, row, policy):
	need(len(row["parents"]) == 2 and row["parents"][1] == policy.engine_tip, "import second parent differs")
	need(git(repo, "rev-parse", row["commit"] + ":rune").strip() == git(repo, "rev-parse", policy.engine_tip + "^{tree}").strip(), "import subtree differs")
	outside = git(repo, "diff-tree", "--no-commit-id", "--name-only", "-r", row["parents"][0], row["commit"]).splitlines()
	need(outside and all(p.startswith("rune/") for p in outside), "import changed outside prefix")
	need(not any(p == "rune" or p.startswith("rune/") for p in paths(repo, row["parents"][0])), "import overwrote existing prefix")


def segment(repo, base, head, policy, engine=False, occupied=None, require_sequence=False):
	git(repo, "merge-base", "--is-ancestor", base, head)
	need(base in git(repo, "rev-list", "--first-parent", head).splitlines(), "cutoff not on complete first-parent chain")
	commits = git(repo, "rev-list", "--reverse", "--first-parent", base + ".." + head).splitlines()
	if require_sequence:
		need(policy.sequence_base == base or policy.sequence_base in commits, "sequence cutoff not on complete first-parent chain")
	previous = base
	previous_tree = paths(repo, base)
	sequenced = not engine and base == policy.sequence_base
	pending = None
	plans, implementations, rows = {}, {}, []
	imported = False
	for commit in commits:
		row = metadata(repo, commit)
		need(row["parents"] and row["parents"][0] == previous, "cutoff not on complete first-parent chain")
		is_import = not engine and commit == policy.import_commit
		if is_import:
			import_proof(repo, row, policy)
			imported = True
		else:
			need(len(row["parents"]) == 1, "unreviewed merge/second-parent history")
		tree = paths(repo, commit)
		changed = git(repo, "diff-tree", "--no-commit-id", "--name-only", "--no-renames", "-r", previous, commit).splitlines()
		kind, number = row["kind"], row["number"]
		if sequenced:
			def sequence_need(condition, rule, message):
				if not condition:
					raise HistoryError(message, commit=commit, rule=rule)
			if pending is None:
				sequence_need(kind == "plan", "sequence.plan", "expected next plan after implementation")
				numbers = [int(m[2]) for path in previous_tree if (m := DOC.fullmatch(path)) and m[1] == "adr"]
				expected = max(numbers, default=205) + 1
				sequence_need(int(number) == expected, "sequence.number", f"expected next record {expected:04d}, got {number}")
				pending = number
			else:
				sequence_need(kind == "impl" and number == pending, "sequence.implementation", f"expected immediate implementation of {pending}")
				pending = None
		follower = not engine and commit in (policy.follower_plan, policy.follower_impl)
		if follower:
			need(number == "0206" and commit == (policy.follower_plan if kind == "plan" else policy.follower_impl), "invalid follower role")
			impl = document(repo, commit, tree, number, "impl")
			need(policy.follower_reference in git(repo, "show", commit + ":" + impl), "missing pinned follower reference")
			allowed = {impl}
		else:
			adr = document(repo, commit, tree, number, "adr")
			impl = document(repo, commit, tree, number, "impl")
			allowed = {adr, impl}
		if sequenced:
			text = git(repo, "show", commit + ":" + adr)
			state = next(line.removeprefix("Status: ") for line in text.splitlines() if line.startswith("Status:"))
			if state.casefold().startswith("withdrawn"):
				sequence_need(state == "Withdrawn" and kind == "impl" and bool(changed) and set(changed) <= allowed,
					"sequence.withdrawal", "withdrawal must be an implementation changing only its paired documents with Status: Withdrawn")
		if kind == "plan":
			need(number not in plans, "duplicate plan")
			need(not occupied or number not in occupied or follower, "shared record number already occupied")
			need(set(changed) == allowed, "plan must change exactly its paired documents")
			plans[number] = commit
		else:
			need(number in plans, "implementation missing earlier first-parent plan")
			implementations[number] = implementations.get(number, 0) + 1
			need(implementations[number] <= MAX_IMPLEMENTATIONS, "implementation limit exceeded")
			if not is_import:
				for path in changed:
					if path.startswith(("plans/", "rune/plans/")):
						need(path in allowed, "implementation changes another record's documents")
			row["plan"] = plans[number]
		# Imported ADRs stay unique; follower implementation plans do not duplicate decisions.
		if imported:
			numbers = [DOC.fullmatch(p)[2] for p in tree if DOC.fullmatch(p) and DOC.fullmatch(p)[1] == "adr" and int(DOC.fullmatch(p)[2]) >= 206]
			need(len(numbers) == len(set(numbers)), "duplicate shared ADR documents")
		row["documents"] = sorted(allowed)
		rows.append(row)
		previous = commit
		previous_tree = tree
		if not engine and commit == policy.sequence_base:
			sequenced = True
	need(previous == head, "empty segment head mismatch")
	return {"base": base, "head": head, "commits": rows, "plans": plans}


def evaluate(repo, policy=Policy()):
	repo = Path(repo).resolve()
	need(git(repo, "rev-parse", "--is-shallow-repository").strip() == "false", "complete history required")
	need(not git(repo, "for-each-ref", "--format=%(refname)", "refs/replace").strip(), "replacement history forbidden")
	grafts = Path(git(repo, "rev-parse", "--git-path", "info/grafts").strip())
	if not grafts.is_absolute():
		grafts = repo / grafts
	need(not grafts.exists(), "grafted history forbidden")
	head = git(repo, "rev-parse", "HEAD^{commit}").strip()
	for value in (policy.host_base, policy.engine_base, policy.engine_tip, policy.import_commit, policy.sequence_base):
		need(bool(re.fullmatch(r"[0-9a-f]{40}", value)), "invalid pinned cutoff")
		git(repo, "cat-file", "-e", value + "^{commit}")
	git(repo, "merge-base", "--is-ancestor", policy.import_commit, head)
	engine = segment(repo, policy.engine_base, policy.engine_tip, policy, engine=True)
	host = segment(repo, policy.host_base, head, policy, occupied=engine["plans"], require_sequence=True)
	return {"status": "PASS", "head": head, "max_implementations": MAX_IMPLEMENTATIONS, "engine": engine, "host": host}


def main():
	p = argparse.ArgumentParser(description=__doc__)
	p.add_argument("--repo", type=Path, default=Path(__file__).resolve().parents[1])
	args = p.parse_args()
	try:
		result = evaluate(args.repo)
	except (HistoryError, OSError, UnicodeError, subprocess.TimeoutExpired) as error:
		result = {"status": "FAIL", "error": str(error)}
		if isinstance(error, HistoryError) and error.commit:
			result.update(commit=error.commit, rule=error.rule)
	print(json.dumps(result, indent=2))
	return 0 if result["status"] == "PASS" else 1


if __name__ == "__main__":
	raise SystemExit(main())
