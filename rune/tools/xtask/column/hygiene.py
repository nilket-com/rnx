"""Reject staged build products before any CK1 build or source pinning."""
from pathlib import PurePosixPath
import subprocess

ARTIFACT_SUFFIXES = {".o", ".obj", ".a", ".so", ".dylib", ".dll", ".exe", ".pdb", ".rlib", ".rmeta", ".pyc"}
ARTIFACT_MAGIC = (b"\x7fELF", b"MZ", b"!<arch>\n", b"\xcf\xfa\xed\xfe", b"\xfe\xed\xfa\xcf", b"\xce\xfa\xed\xfe", b"\xfe\xed\xfa\xce")


def artifact_path(name):
	path = PurePosixPath(name)
	return path.parts[0:1] == ("tools",) and ("target" in path.parts or path.suffix in ARTIFACT_SUFFIXES)


def check(root):
	probe_target = "tools/xtask/column/probe/target/ck1-ignore-control"
	if subprocess.run(["git", "check-ignore", "--quiet", probe_target], cwd=root).returncode:
		raise ValueError("CK1 probe target directory must be gitignored")
	staged = subprocess.check_output(["git", "diff", "--cached", "--name-only", "--diff-filter=ACMR", "-z"], cwd=root).decode().split("\0")
	for name in filter(None, staged):
		if not name.startswith("tools/"):
			continue
		if artifact_path(name):
			raise ValueError(f"staged build artifact: {name}")
		# Inspect the index object rather than the working copy: replacing a
		# staged binary on disk must not let the staged binary pass this guard.
		with subprocess.Popen(["git", "show", ":" + name], cwd=root, stdout=subprocess.PIPE, stderr=subprocess.DEVNULL) as process:
			prefix = process.stdout.read(8)
			process.stdout.close()
			process.wait()
		if any(prefix.startswith(magic) for magic in ARTIFACT_MAGIC):
			raise ValueError(f"staged executable/archive under tools: {name}")


def controls():
	for name in ("tools/xtask/column/probe/target/release/probe", "tools/other/target/a.rs", "tools/unexpected.o", "tools/lib.rlib", "tools/kernel.so"):
		assert artifact_path(name), name
	for name in ("tools/xtask/column/probe/build.rs", "tools/xtask/column/probe/src/main.rs", "tools/xtask/column/dependencies.json", "tools/target_description.rs"):
		assert not artifact_path(name), name


def index_controls():
	"""A replaced working file must not conceal a staged binary."""
	import tempfile
	from pathlib import Path
	with tempfile.TemporaryDirectory(prefix="ck1-index-control-") as directory:
		root = Path(directory)
		subprocess.run(["git", "init", "--quiet", str(root)], check=True)
		(root / ".gitignore").write_text("/tools/xtask/column/probe/target\n")
		(root / "tools").mkdir()
		file = root / "tools/extensionless"
		file.write_bytes(b"\x7fELF\x00\x00\x00\x00")
		subprocess.run(["git", "add", "tools/extensionless"], cwd=root, check=True)
		file.write_text("ordinary source in the working copy\n")
		try:
			check(root)
		except ValueError as error:
			assert "staged executable" in str(error)
		else:
			raise AssertionError("staged binary hidden by working-tree replacement")
		subprocess.run(["git", "add", "tools/extensionless"], cwd=root, check=True)
		check(root)
