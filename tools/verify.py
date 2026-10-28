#!/usr/bin/env python3
"""Prepare a private Cargo graph and launch the Rust cross-repository verifier.

Observation, source binding, comparison, and acceptance belong to the checked-in
Rust controller. This launcher never changes the product manifest or lock.
"""

import argparse
import json
import os
import re
from pathlib import Path
import shutil
import subprocess
import tarfile
import time
import tomllib



def check_rune_pins(root):
    """Check every live graph; probes/ records historical, independently pinned work."""
    root = Path(root)
    dependency = tomllib.loads((root / "Cargo.toml").read_text())["dependencies"]["rune"]
    revision, url = dependency["rev"], dependency["git"]
    if re.fullmatch(r"[0-9a-f]{40}", revision) is None:
        raise ValueError("the Rune pin must be a full Git revision")
    expected = f"git+{url}?rev={revision}#{revision}"
    checked = {"revision": revision, "locks": [], "manifests": [], "versions": []}

    def dependencies(value):
        if isinstance(value, dict):
            if value.get("git") == url:
                yield value
            for child in value.values():
                yield from dependencies(child)
        elif isinstance(value, list):
            for child in value:
                yield from dependencies(child)

    for directory, folders, files in os.walk(root, followlinks=False):
        here = Path(directory)
        folders[:] = sorted(name for name in folders
                            if name not in {".git", "target", "__pycache__"}
                            and not (here == root and name == "probes")
                            and not (here / name).is_symlink())
        for name in sorted(set(files) & {"Cargo.lock", "Cargo.toml"}):
            path = here / name
            relative = path.relative_to(root).as_posix()
            document = tomllib.loads(path.read_text())
            if name == "Cargo.toml":
                pins = list(dependencies(document))
                if any(pin.get("rev") != revision for pin in pins):
                    raise ValueError(f"{relative}: Rune manifest revision differs from {revision}")
                if pins:
                    checked["manifests"].append(relative)
            else:
                packages = [package for package in document.get("package", [])
                            if package["name"] == "rune" or package["name"].startswith("rune-")]
                for package in packages:
                    if package.get("source") != expected:
                        raise ValueError(f"{relative}: {package['name']} source differs from {expected}")
                if packages:
                    checked["locks"].append(relative)
    for relative in ("Cargo.lock", "tools/project/Cargo.lock"):
        if relative not in checked["locks"]:
            raise ValueError(f"{relative}: missing Rune lock entry")
    for relative in ("src/lib.rs", "jupyter/src/worker.rs"):
        text = (root / relative).read_text()
        versions = re.findall(r'const RUNE_VERSION:\s*&str\s*=\s*"[^"\n]*\(git ([0-9a-f]{40})\)"', text)
        if versions != [revision]:
            raise ValueError(f"{relative}: RUNE_VERSION differs from {revision}")
        checked["versions"].append(relative)
    return checked


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("command", nargs="?", default="cross", choices=["cross"])
    parser.add_argument("--suite", choices=["census", "enrollment", "all"], default="all")
    source = parser.add_mutually_exclusive_group()
    source.add_argument("--rune-rev")
    source.add_argument("--rune-path", type=Path)
    parser.add_argument("--rune-repo", type=Path)
    parser.add_argument("--fixtures-root", type=Path, help="override the checked-in test input root")
    parser.add_argument("--out", type=Path)
    available_cpus = len(os.sched_getaffinity(0))
    parser.add_argument("--jobs", type=int, default=min(8, available_cpus))
    parser.add_argument("--registration-fixture", type=Path)
    args = parser.parse_args()
    if not shutil.which("bwrap"):
        parser.error("bwrap (bubblewrap) is required; no host-filesystem fallback is available")
    cargo_path = shutil.which("cargo")
    if not cargo_path:
        parser.error("cargo is required on PATH (the runner uses toolchain 1.98.1)")
    version = subprocess.check_output(["bwrap", "--version"], text=True).strip()
    if not 1 <= args.jobs <= available_cpus:
        parser.error(f"--jobs must be between 1 and the {available_cpus} CPUs in the current affinity")
    controller_root = Path(__file__).resolve().parents[1]
    try:
        pin_consistency = check_rune_pins(controller_root)
    except (ValueError, KeyError, OSError) as error:
        parser.error(str(error))
    root = (args.registration_fixture.resolve(strict=True)
            if args.registration_fixture else controller_root)
    args.fixtures_root = (args.fixtures_root or root / "tools/verify/fixtures").resolve(strict=True)
    if args.rune_path:
        args.rune_path = args.rune_path.resolve(strict=True)
    if args.out is None:
        args.out = root / "target" / "rnx-verify" / f"{os.getpid()}-{time.time_ns()}"
    output = args.out.resolve()
    if output.is_relative_to(root):
        scratch_root = (root / "target" / "rnx-verify").resolve()
        if not output.is_relative_to(scratch_root):
            parser.error("in-tree output must be under target/rnx-verify")
        ignored = subprocess.run(["git", "-C", str(root), "check-ignore", "-q", "--", str(output.relative_to(root))])
        if ignored.returncode != 0:
            parser.error("in-tree output must be ignored by Git")
    if args.rune_path and output.is_relative_to(args.rune_path):
        parser.error("--out must be outside the Rune source tree")
    output.mkdir(parents=True, exist_ok=False)
    with (output / "PIN-CONSISTENCY.json").open("x") as stream:
        json.dump(pin_consistency, stream, indent=2)
        stream.write("\n")
    manifest = output / "Cargo.toml"
    lock = output / "Cargo.lock"
    original_manifest = output / "original-Cargo.toml"
    original_lock = output / "original-Cargo.lock"
    shutil.copyfile(root / "Cargo.toml", original_manifest)
    shutil.copyfile(root / "Cargo.lock", original_lock)
    rune_repo = args.rune_repo.resolve() if args.rune_repo else None
    if not args.rune_rev and not args.rune_path:
        # The default is the locked Git dependency, obtained by Cargo in its
        # declared cache. No sibling checkout participates in this path.
        package_manifest = tomllib.loads((root / "Cargo.toml").read_text())
        dependency = package_manifest["dependencies"]["rune"]
        rev, url = dependency["rev"], dependency["git"]
        if len(rev) != 40 or any(c not in "0123456789abcdef" for c in rev):
            parser.error("the Rune dependency must have a full Git commit")
        metadata = json.loads(subprocess.check_output(
            [cargo_path, "+1.98.1", "metadata", "--locked", "--format-version=1"], cwd=root))
        selected = [p for p in metadata["packages"] if p["name"] == "rune"]
        if len(selected) != 1 or selected[0]["source"] != f"git+{url}?rev={rev}#{rev}":
            parser.error("Cargo resolved a Rune dependency other than the pinned commit")
        path = Path(selected[0]["manifest_path"]).resolve(strict=True)
        if path.parts[-3:] != ("crates", "rune", "Cargo.toml"):
            parser.error("unexpected Rune package layout")
        cargo_checkout = path.parents[2]
        checked_rev = subprocess.check_output(["git", "-C", str(cargo_checkout), "rev-parse", "HEAD"], text=True).strip()
        if checked_rev != rev:
            parser.error("Cargo's Rune checkout does not point at the pinned commit")
        # Export the pinned Git object into the scratch job. Cargo's marker
        # files and any other checkout-local state are not source inputs.
        rune_repo = cargo_checkout
        args.rune_rev = rev
    if args.rune_rev and rune_repo is None:
        parser.error("--rune-rev requires --rune-repo")
    if args.rune_rev:
        resolved = subprocess.check_output(
            ["git", "-C", str(rune_repo), "rev-parse", args.rune_rev + "^{commit}"],
            text=True).strip()
        rune = output / "rune"
        rune.mkdir()
        archive = output / "rune.tar"
        with archive.open("xb") as stream:
            subprocess.run(["git", "-C", str(rune_repo), "archive", args.rune_rev],
                           stdout=stream, check=True)
        with tarfile.open(archive) as stream:
            stream.extractall(rune, filter="data")
    else:
        resolved = None
        rune = args.rune_path.resolve(strict=True)
    if output.is_relative_to(rune):
        parser.error("--out must be outside the Rune source tree")
    text = (root / "Cargo.toml").read_text()
    text = text.replace('[package]\n', '[package]\nautolib = false\nautobins = false\nautotests = false\nautoexamples = false\nautobenches = false\n', 1)
    text = text.replace('[features]\n', '[features]\ncross-verification = []\n', 1)
    text += '\n[[test]]\nname = "rnx"\npath = "src/lib.rs"\nharness = false\nrequired-features = ["cross-verification"]\n'
    text += '\n[patch."https://github.com/nilket-com/rune"]\n'
    packages = ["rune", "rune-core", "rune-alloc", "rune-alloc-macros",
                "rune-macros", "rune-tracing", "rune-tracing-macros"]
    for package in packages:
        text += f"{package} = {{ path = {json.dumps(str(rune / 'crates' / package))} }}\n"
    manifest.write_text(text)
    shutil.copyfile(root / "Cargo.lock", lock)
    request = dict(vars(args))
    request.update(rnx_root=root, rune_root=rune, rune_repo=rune_repo,
                   controller_root=controller_root,
                   out=output, bwrap_version=version,
                   original_manifest=original_manifest, original_lock=original_lock,
                   rune_revision_resolved=resolved,
                   rune_archive=output / "rune.tar" if args.rune_rev else None)
    request_path = output / "REQUEST.json"
    request_path.write_text(json.dumps(request, default=str, indent=2) + "\n")
    env = dict(os.environ, CARGO_TARGET_DIR=str(output / "target"),
               RNX_VERIFY_RUNE_ROOT=str(rune),
               CARGO_BUILD_JOBS=str(args.jobs), CARGO_INCREMENTAL="0",
               CARGO_PROFILE_DEV_DEBUG="2", CARGO_PROFILE_TEST_DEBUG="2",
               CARGO_PROFILE_DEV_STRIP="none", CARGO_PROFILE_TEST_STRIP="none",
               RUSTFLAGS="-Cdebuginfo=2 -Cstrip=none")
    env.pop("CARGO_ENCODED_RUSTFLAGS", None)
    cargo_home_env = os.environ.get("CARGO_HOME")
    cargo_home = (Path(cargo_home_env) if cargo_home_env else Path.home() / ".cargo").resolve(strict=True)
    namespace = ["bwrap", "--die-with-parent", "--ro-bind", "/", "/",
                 "--tmpfs", "/tmp",
                 "--ro-bind", str(root), str(root),
                 "--bind", str(output), str(output),
                 "--ro-bind", str(rune), str(rune),
                 "--bind", str(cargo_home), str(cargo_home),
                 "--ro-bind", str(manifest), str(root / "Cargo.toml"),
                 "--bind", str(lock), str(root / "Cargo.lock"),
                 "--proc", "/proc", "--dev", "/dev", "--chdir", str(root)]
    cargo = [cargo_path, "+1.98.1"]
    # Resolve only the private lock. The Rust controller validates all seven
    # packages, including the compiler-only project dependency.
    with (output / "metadata.json").open("xb") as stdout, (output / "metadata.stderr").open("xb") as stderr:
        subprocess.run(namespace + cargo + ["metadata", "--offline", "--format-version", "1",
                                           "--features", "cross-verification"],
                       env=env, stdout=stdout, stderr=stderr, check=True)
    command = namespace + cargo + ["test", "--offline", "--locked", "--test", "rnx",
                                   "--features", "cross-verification", "--no-run",
                                   "--message-format=json"]
    with (output / "cargo.stdout").open("xb") as stdout, (output / "cargo.stderr").open("xb") as stderr:
        subprocess.run(command, env=env, stdout=stdout, stderr=stderr, check=True)
    # This locates a launch candidate only. Rust independently validates the
    # entire artifact stream, uniqueness and source/binary identity before use.
    records = (json.loads(line) for line in (output / "cargo.stdout").read_text().splitlines())
    executable = next(row["executable"] for row in records
                      if row.get("reason") == "compiler-artifact"
                      and row.get("target", {}).get("name") == "rnx"
                      and row.get("target", {}).get("kind") == ["test"]
                      and row.get("executable"))
    try:
        with (output / "controller.stdout").open("xb") as stdout, (output / "controller.stderr").open("xb") as stderr:
            subprocess.run([executable, "--controller", str(request_path)],
                           env=env, stdout=stdout, stderr=stderr, check=True)
    finally:
        binary = output / "bound-verification-child"
        if binary.exists():
            with (output / "host-check.stdout").open("xb") as stdout, (output / "host-check.stderr").open("xb") as stderr:
                subprocess.run([str(binary), "--host-check", str(request_path)],
                               env=env, stdout=stdout, stderr=stderr, check=True)
    if not (output / "bound-verification-child").is_file():
        raise RuntimeError("the Rust controller did not bind an executable; see controller.stderr")


if __name__ == "__main__":
    main()
