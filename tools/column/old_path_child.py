"""One blocking GDB/Callgrind run, started under the measurement lease."""
import json
import os
from pathlib import Path
import subprocess
import sys

config_path = Path(sys.argv[1]).resolve()
config = json.loads(config_path.read_text())
if config.get("canonical_layout"):
	layout = Path("/run/rnx-route")
	if sys.argv[2:] != ["--inside-layout"]:
		command = ["/usr/bin/bwrap", "--die-with-parent", "--ro-bind", "/", "/",
			"--dev-bind", "/dev", "/dev", "--proc", "/proc", "--tmpfs", "/tmp",
			"--tmpfs", "/run", "--dir", str(layout)]
		for source, name in [(config["binary"], "rnx"), (config["fixture"], "program.rn"),
				(config["valgrind_root"], "valgrind"), (__file__, "old_path_child.py"),
				(Path(__file__).with_name("old_path_gdb.py"), "old_path_gdb.py")]:
			command.extend(["--ro-bind", str(source), str(layout / name)])
		command.extend(["--bind", config["out"], str(layout / "out"), "--chdir", str(layout),
			"--", "/usr/bin/env", "-i", *(f"{k}={v}" for k, v in os.environ.items()),
			f"VALGRIND_LIB={layout}/valgrind/libexec/valgrind", "/usr/bin/python3", "-B",
			str(layout / "old_path_child.py"), str(layout / "out/CONFIG.json"), "--inside-layout"])
		os.execv(command[0], command)
	config.update(binary=str(layout / "rnx"), fixture=str(layout / "program.rn"),
		out=str(layout / "out"), valgrind_root=str(layout / "valgrind"))
	config_path = layout / "out/OBSERVER.json"
	config["gdb_receipt"] = str(layout / "out/GDB.json")
	with config_path.open("x") as observer:
		json.dump(config, observer)
out = Path(config["out"])
vgroot = Path(config["valgrind_root"])
prefix = str(out / "vgdb")
argv = [str(vgroot / "bin/valgrind"), "--tool=callgrind", "--instr-atstart=no", "--collect-atstart=yes",
	"--separate-threads=no", "--collect-jumps=yes", "--dump-instr=yes", "--cache-sim=no", "--branch-sim=no",
	"--vgdb=yes", "--vgdb-error=0", "--vgdb-prefix=" + prefix, "--callgrind-out-file=" + str(out / "map.callgrind"),
	config["binary"], "run", "--debug-engine", *config["options"], "--budget", str(config.get("budget", 1000000)), config["fixture"]]
environment = os.environ.copy()
if config.get("canonical_layout") or config.get("phase_symbols"):
	# Bind exactly the argument and environment order passed to exec.
	context = {"argv": argv, "cwd": os.getcwd(), "environ": [k + "=" + v for k, v in environment.items()]}
	with (out / "LAUNCH.json").open("x", encoding="ascii") as observed:
		json.dump(context, observed, sort_keys=True, separators=(",", ":"), ensure_ascii=True)
with (out / "program.stdout").open("x") as stdout, (out / "program.stderr").open("x") as stderr:
	process = subprocess.Popen(argv, stdout=stdout, stderr=stderr, env=environment)
	try:
		commands = out / "commands.gdb"
		commands.write_text("\n".join([
			"set pagination off", "set confirm off", "set print thread-events off", "set language c",
			"file " + config["binary"],
			"target remote | " + str(vgroot / "bin/vgdb") + " --wait=60 --pid=" + str(process.pid) + " --vgdb-prefix=" + prefix,
			"source " + str(Path(__file__).with_name("old_path_gdb.py")), "continue", "",
		]))
		with (out / "gdb.stdout").open("x") as gout, (out / "gdb.stderr").open("x") as gerr:
			done = subprocess.run(["/usr/bin/gdb", "--nx", "--batch", "-x", str(commands)],
				env=dict(os.environ, COLUMN_BOUNDARY_CONFIG=str(config_path)), stdout=gout, stderr=gerr, timeout=600)
		assert done.returncode == 0, "GDB failed"
		assert process.wait(timeout=60) == 0, "CLI failed"
		assert json.loads(Path(config["gdb_receipt"]).read_text())["status"] == "PASS", "boundary failed"
	finally:
		if process.returncode is None:
			process.kill()
			process.wait()
