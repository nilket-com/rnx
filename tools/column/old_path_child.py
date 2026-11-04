"""One blocking GDB/Callgrind run, started under the measurement lease."""
import json
import os
from pathlib import Path
import subprocess
import sys

config_path = Path(sys.argv[1]).resolve()
config = json.loads(config_path.read_text())
out = Path(config["out"])
vgroot = Path(config["valgrind_root"])
prefix = str(out / "vgdb")
argv = [str(vgroot / "bin/valgrind"), "--tool=callgrind", "--instr-atstart=no", "--collect-atstart=yes",
	"--separate-threads=no", "--collect-jumps=yes", "--dump-instr=yes", "--cache-sim=no", "--branch-sim=no",
	"--vgdb=yes", "--vgdb-error=0", "--vgdb-prefix=" + prefix, "--callgrind-out-file=" + str(out / "map.callgrind"),
	config["binary"], "run", "--debug-engine", *config["options"], "--budget", str(config.get("budget", 1000000)), config["fixture"]]
with (out / "program.stdout").open("x") as stdout, (out / "program.stderr").open("x") as stderr:
	process = subprocess.Popen(argv, stdout=stdout, stderr=stderr)
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
