"""Executed inside GDB; no calls into the measured program."""
import gdb
import json
import os
from pathlib import Path

config = json.loads(Path(os.environ["COLUMN_BOUNDARY_CONFIG"]).read_text())
state = {"status": "FAIL", "entries": [], "returns": [], "workers": [], "errors": []}
active = False


def save():
	Path(config["gdb_receipt"]).write_text(json.dumps(state, indent=2) + "\n")


class Return(gdb.FinishBreakpoint):
	def stop(self):
		global active
		try:
			gdb.execute("monitor dump column-map")
			gdb.execute("monitor instrumentation off")
			state["returns"].append(gdb.selected_thread().global_num)
			active = False
			save()
			return False
		except BaseException as error:
			state["errors"].append(repr(error))
			save()
			return True

	def out_of_scope(self):
		state["errors"].append("map returned without FinishBreakpoint")
		save()


class Entry(gdb.Breakpoint):
	def stop(self):
		global active
		try:
			assert not active, "unexpected nested map"
			state["entries"].append(gdb.selected_thread().global_num)
			Return(gdb.newest_frame(), internal=True)
			gdb.execute("monitor instrumentation on")
			gdb.execute("monitor zero")
			active = True
			return False
		except BaseException as error:
			state["errors"].append(repr(error))
			save()
			return True


class Worker(gdb.Breakpoint):
	def stop(self):
		if not active:
			state["errors"].append("worker outside map boundary")
		state["workers"].append(gdb.selected_thread().global_num)
		return False


def exited(event):
	state["exit"] = event.exit_code
	if event.exit_code == 0 and not state["errors"] and not active:
		count = config["calls"]
		if len(state["entries"]) == len(state["returns"]) == len(state["workers"]) == count and all(t not in state["entries"] for t in state["workers"]):
			state["status"] = "PASS"
	save()


gdb.events.exited.connect(exited)
Entry("*'" + config["symbols"]["map"] + "'", internal=True)
Worker("*'" + config["symbols"]["worker"] + "'", internal=True)
