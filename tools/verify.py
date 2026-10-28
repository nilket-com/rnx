#!/usr/bin/env python3
"""Prepare a private Cargo graph and launch the Rust cross-repository verifier.

Observation, source binding, comparison, and acceptance belong to the checked-in
Rust controller. This launcher never changes the product manifest or lock.
"""

import argparse
import json
import os
from pathlib import Path
import shutil
import subprocess
import tarfile


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("command", choices=["cross"])
    parser.add_argument("--suite", choices=["census", "enrollment", "all"], required=True)
    source = parser.add_mutually_exclusive_group(required=True)
    source.add_argument("--rune-rev")
    source.add_argument("--rune-path", type=Path)
    parser.add_argument("--rune-repo", type=Path)
    parser.add_argument("--fixtures-root", type=Path, required=True)
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument("--jobs", type=int, default=8)
    parser.add_argument("--registration-fixture", type=Path)
    args = parser.parse_args()
    if not shutil.which("bwrap"):
        parser.error("bwrap (bubblewrap) is required; no host-filesystem fallback is available")
    cargo_path = shutil.which("cargo")
    if not cargo_path:
        parser.error("cargo is required on PATH (the runner uses toolchain 1.98.1)")
    version = subprocess.check_output(["bwrap", "--version"], text=True).strip()
    if not 1 <= args.jobs <= 30:
        parser.error("--jobs must be between 1 and 30; shared CPU allocation is caller-owned")
    controller_root = Path(__file__).resolve().parents[1]
    root = (args.registration_fixture.resolve(strict=True)
            if args.registration_fixture else controller_root)
    args.fixtures_root = args.fixtures_root.resolve(strict=True)
    if args.rune_path:
        args.rune_path = args.rune_path.resolve(strict=True)
    output = args.out.resolve()
    if output.is_relative_to(root) or (args.rune_path and output.is_relative_to(args.rune_path)):
        parser.error("--out must be outside both source trees")
    output.mkdir(parents=True, exist_ok=False)
    manifest = output / "Cargo.toml"
    lock = output / "Cargo.lock"
    original_manifest = output / "original-Cargo.toml"
    original_lock = output / "original-Cargo.lock"
    shutil.copyfile(root / "Cargo.toml", original_manifest)
    shutil.copyfile(root / "Cargo.lock", original_lock)
    rune_repo = (args.rune_repo or controller_root.parent / "rune").resolve()
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
    namespace = ["bwrap", "--die-with-parent", "--ro-bind", "/", "/",
                 "--tmpfs", "/tmp",
                 "--bind", str(output), str(output),
                 "--ro-bind", str(root), str(root),
                 "--ro-bind", str(rune), str(rune),
                 "--bind", str(Path.home() / ".cargo"), str(Path.home() / ".cargo"),
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
