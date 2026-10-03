"""Records 0156 and 0157: checks the notebook showcase (plans/0156 sections 3, 4, 5a; plans/0157).

	python check.py --rnx /abs/rnx --kernel /abs/rnx-jupyter [--polars /abs/rnx-polars-demo]            verify (default)
	python check.py --rnx /abs/rnx --kernel /abs/rnx-jupyter [--polars /abs/rnx-polars-demo] --generate write the committed outputs
	python check.py --rnx /abs/rnx --kernel /abs/rnx-jupyter [--polars /abs/rnx-polars-demo] --controls run the comparison controls

Without --polars, the plain notebooks (01-03) run on plain rnx. With --polars, the Polars worker
built from demos/polars/ also runs 04_polars_sales and, again, the plain notebooks: it is a
superset of plain rnx.

Run it with the pinned environment in requirements.txt. A temporary kernelspec in a
temporary JUPYTER_PATH / JUPYTER_DATA_DIR points at the given executables; the user's
own kernel registry is never read or changed. Verification never writes a committed file.

Per notebook, in a fresh kernel, all cells run. Any `error` output, or any stderr stream,
fails, whatever the client's error setting. The outputs are compared with the committed
ones after coalescing only adjacent same-name stream records within one cell (transport
chunking); execution counts, timestamps, ids and metadata are not compared, and no
output text or order is normalized. The notebook's stdout, concatenated, must equal the
terminal demo run live with the notebook's data path, and that live run must equal the
committed transcript demos/out/NN.txt (for 02, except the data path on its first line).
Each notebook runs twice in fresh kernels; one also restarts its own kernel and reruns
from the first cell.

04_polars_sales has no transcript of its own: its stdout must equal the worker running
demos/polars/sales.rn on the same CSV, and every frame it displays, and its answer lines, must
equal expectations computed here from demos/data/sales.csv with the csv module alone."""
import argparse, copy, csv, hashlib, html.parser, itertools, json, math, os, pathlib, re, shutil, struct, subprocess, sys, tempfile

import nbclient
import nbformat
from nbclient.util import run_sync

HERE = pathlib.Path(__file__).resolve().parent
DEMOS = HERE.parent
NOTEBOOKS = {"01_mortgage": [], "02_orders": ["../data/orders.json"], "03_report": ["../data/orders.json"]}
POLARS_NOTEBOOK = "04_polars_sales"
SALES = DEMOS / "data" / "sales.csv"


_TEMPORARY = []


class Mismatch(Exception):
	pass


def kernelspecs(workers, kernel):
	"""One temporary kernelspec per worker, {name: executable}, in one temporary Jupyter root."""
	holder = tempfile.TemporaryDirectory(prefix="rnx-0156-jupyter-")
	_TEMPORARY.append(holder)  # removed when the checker exits
	root = pathlib.Path(holder.name)
	for name, exe in workers.items():
		spec = root / "kernels" / name
		spec.mkdir(parents=True)
		spec.joinpath("kernel.json").write_text(json.dumps({
			"argv": [kernel, "--connection-file", "{connection_file}", "--rnx", exe],
			"display_name": f"Rune ({name})", "language": "rune"}))
	os.environ["JUPYTER_PATH"] = str(root)
	os.environ["JUPYTER_DATA_DIR"] = str(root)
	os.environ["JUPYTER_CONFIG_DIR"] = str(root / "config")
	os.environ["JUPYTER_RUNTIME_DIR"] = str(root / "runtime")


def outputs(cell):
	"""A cell's outputs as comparable records; only adjacent same-name streams coalesce."""
	out = []
	for o in cell.get("outputs", []):
		if o["output_type"] == "stream":
			if out and out[-1][0] == "stream" and out[-1][1] == o["name"]:
				out[-1] = ("stream", o["name"], out[-1][2] + o["text"])
			else:
				out.append(("stream", o["name"], o["text"]))
		elif o["output_type"] in ("execute_result", "display_data"):
			out.append((o["output_type"], json.dumps(o.get("data", {}), sort_keys=True)))
		elif o["output_type"] == "error":
			out.append(("error", o.get("ename", ""), o.get("evalue", "")))
		else:
			out.append((o["output_type"], json.dumps(o, sort_keys=True)))
	return out


def judge(nb):
	"""Fails on any error output or stderr stream; returns the concatenated stdout."""
	stdout = ""
	for i, cell in enumerate(nb.cells):
		for rec in outputs(cell):
			if rec[0] == "error":
				raise Mismatch(f"cell {i}: error output {rec[1]}: {rec[2]}")
			if rec[0] == "stream" and rec[1] != "stdout":
				raise Mismatch(f"cell {i}: unexpected {rec[1]} output {rec[2]!r}")
			if rec[0] == "stream":
				stdout += rec[2]
	return stdout


def compare(stored, ran):
	if len(stored.cells) != len(ran.cells):
		raise Mismatch("cell count differs")
	for i, (a, b) in enumerate(zip(stored.cells, ran.cells)):
		if a.cell_type != b.cell_type or a.source != b.source:
			raise Mismatch(f"cell {i}: source differs")
		if a.cell_type == "code" and outputs(a) != outputs(b):
			raise Mismatch(f"cell {i}: outputs differ:\n  stored {outputs(a)}\n  ran    {outputs(b)}")


def execute(nb, kernel_name, restart=False):
	"""Runs all cells in a fresh kernel; with restart, then restarts that same kernel and
	reruns from the first code cell, returning both runs."""
	first = copy.deepcopy(nb)
	for cell in first.cells:
		if cell.cell_type == "code":
			cell.outputs, cell.execution_count = [], None
	client = nbclient.NotebookClient(first, timeout=120, kernel_name=kernel_name, allow_errors=True,
									 resources={"metadata": {"path": str(HERE)}})
	with client.setup_kernel():
		for i, cell in enumerate(first.cells):
			client.execute_cell(cell, i)
		if not restart:
			return [first]
		second = copy.deepcopy(nb)
		for cell in second.cells:
			if cell.cell_type == "code":
				cell.outputs, cell.execution_count = [], None
		# the same client-managed kernel is restarted (nbclient's manager is async)
		marker = nbformat.v4.new_code_cell("let restart_marker = 1;")
		first.cells.append(marker)
		client.execute_cell(marker, len(first.cells) - 1)
		first.cells.pop()
		if any(o["output_type"] == "error" for o in marker.outputs):
			raise Mismatch("the restart marker could not be set")
		before = client.km.provisioner.pid
		run_sync(client.km.restart_kernel)(now=False)
		run_sync(client.kc.wait_for_ready)(timeout=60)
		after = client.km.provisioner.pid
		if before is None or after is None or before == after:
			raise Mismatch(f"the kernel process did not restart (pid {before} -> {after})")
		client.nb = second
		probe = nbformat.v4.new_code_cell("restart_marker")
		second.cells.append(probe)
		client.execute_cell(probe, len(second.cells) - 1)
		second.cells.pop()
		if not any(o["output_type"] == "error" for o in probe.outputs):
			raise Mismatch("state from before the restart survived it")
		for i, cell in enumerate(second.cells):
			client.execute_cell(cell, i)
		return [first, second]


def terminal(rnx, name):
	if name == POLARS_NOTEBOOK:
		return polars_terminal(rnx, SALES)
	if name in CANDLE_NOTEBOOKS:
		return candle_terminal(rnx, name)
	args = NOTEBOOKS[name]
	r = subprocess.run([rnx, "run", f"../{name}.rn", *args], cwd=HERE, capture_output=True, text=True)
	if r.returncode != 0 or r.stderr:
		raise Mismatch(f"{name}: the terminal demo failed: exit {r.returncode}, stderr {r.stderr!r}")
	transcript = (DEMOS / "out" / f"{name}.txt").read_text()
	live = r.stdout
	if args:
		lines = live.split("\n")
		if args[0] in lines[0]:
			lines[0] = lines[0].replace(args[0], "demos/data/orders.json")
		live = "\n".join(lines)
	if live != transcript:
		raise Mismatch(f"{name}: the live terminal run differs from demos/out/{name}.txt beyond the data path")
	return r.stdout


def verify(rnx, kernel_name, name, restart):
	path = HERE / f"{name}.ipynb"
	stored = nbformat.read(path, as_version=4)
	if CANDLE_NOTEBOOKS.get(name):
		model_ready()
	reference = terminal(rnx, name)
	if name == POLARS_NOTEBOOK:
		check_sales_answer(reference, sales_expected(SALES))
		check_sales(stored, SALES)
	if name in CANDLE_NOTEBOOKS:
		check_candle(name, stored)
	runs = execute(stored, kernel_name, restart=restart) + execute(stored, kernel_name)
	for run in runs:
		stdout = judge(run)
		if stdout != reference:
			raise Mismatch(f"{name}: the notebook's stdout differs from the terminal demo:\n{stdout!r}\n{reference!r}")
		compare(stored, run)
		if name == POLARS_NOTEBOOK:
			check_sales(run, SALES)
		if name in CANDLE_NOTEBOOKS:
			check_candle(name, run)
	return len(runs)


# ---- 04_polars_sales: independent expectations (plans/0157 section 5 and 5a) ----------------

def polars_terminal(worker, data):
	"""The worker runs the terminal script with absolute paths, from the checkout root."""
	r = subprocess.run([worker, "run", str(DEMOS / "polars" / "sales.rn"), str(data)],
					   cwd=DEMOS.parent, capture_output=True, text=True)
	if r.returncode != 0 or r.stderr:
		raise Mismatch(f"sales.rn failed: exit {r.returncode}, stderr {r.stderr!r}")
	return r.stdout


SALES_SCHEMA = [("month", "string"), ("region", "string"), ("product", "string"),
				("units", "i64"), ("price_cents", "i64"), ("returned", "bool")]


def sales_expected(data):
	"""Every displayed frame and the answer, from the CSV with the csv module, in integer cents."""
	with open(data, newline="") as f:
		reader = csv.reader(f)
		if next(reader) != [n for n, _ in SALES_SCHEMA]:
			raise Mismatch("sales.csv: unexpected header")
		rows = []
		for month, region, product, units, price, returned in reader:
			if returned not in ("true", "false"):
				raise Mismatch(f"sales.csv: returned is {returned!r}")
			rows.append([month, region, product, int(units), int(price), returned == "true"])
	kept = [r + [r[3] * r[4]] for r in rows if not r[5]]
	regions, months = {}, {}
	for r in kept:
		g = regions.setdefault(r[1], [r[1], 0, 0, 0])
		g[1] += r[3]
		g[2] += r[6]
		g[3] += 1
		months[r[0]] = months.get(r[0], 0) + r[6]
	by_region = sorted(regions.values(), key=lambda g: -g[2])
	if len({g[2] for g in by_region}) != len(by_region):
		raise Mismatch("sales.csv: tied region revenue makes the sorted order ambiguous")
	total = sum(r[6] for r in kept)
	top = by_region[0]
	tenths = (top[2] * 2000 + total) // (2 * total)
	return {
		"frames": [
			("sales", len(rows), SALES_SCHEMA, rows),
			("kept", len(kept), SALES_SCHEMA + [("revenue_cents", "i64")], kept),
			("by_region", len(by_region), [("region", "string"), ("units", "i64"),
										   ("revenue_cents", "i64"), ("orders", "u32")], by_region),
			("by_month", len(months), [("month", "string"), ("revenue_cents", "i64")],
			 [[m, months[m]] for m in sorted(months)]),
			# the mean is an f64 of total / count; for this data it is exact (6137.5), so the
			# displayed value must equal it exactly, not within a tolerance
			("summary", 1, [("total_cents", "i64"), ("mean_cents", "f64")], [[total, total / len(kept)]]),
		],
		"answer": (total, len(kept), len(rows) - len(kept), top[0], top[2], tenths),
	}


ROWS_SHOWN = 10
HALF = ROWS_SHOWN // 2


SIMPLE = r"[A-Za-z0-9_-]+"
"""04's string cells. 06's titles also have spaces and apostrophes (TITLES); nothing else widens."""
TITLES = r"[A-Za-z0-9_'-]+(?: [A-Za-z0-9_'-]+)*"


def f32(x):
	"""The nearest float32 to x, as a Python float."""
	return struct.unpack("<f", struct.pack("<f", x))[0]


def parse_frame(text, whole, strings=SIMPLE):
	"""This example's previews only (record 0158's table layout): at most 8 columns; string,
	integer, float and bool cells; no truncated scalars. Elision is read structurally, from the
	shape: a frame taller than 10 rows shows its first and last 5 rows with one elided row
	between, and a `…` anywhere else is refused. Returns (height, width, schema, cells)."""
	lines = text.split("\n")
	if lines[-1] != "":
		raise Mismatch(f"preview does not end in a newline: {text!r}")
	lines.pop()
	if "[preview byte limit" in text:
		raise Mismatch(f"the preview hit its byte limit: {text!r}")
	m = re.fullmatch(r"shape: \((\d[\d_]*), (\d[\d_]*)\)", lines.pop(0)) if lines else None
	if not m:
		raise Mismatch(f"no shape line: {text!r}")
	height, width = int(m[1].replace("_", "")), int(m[2].replace("_", ""))
	if not 0 < width <= 8:
		raise Mismatch(f"{width} columns is outside this example")
	top = re.fullmatch(r"┌((?:─+┬)*─+)┐", lines[0]) if lines else None
	if not top:
		raise Mismatch(f"no top border: {text!r}")
	widths = [len(s) - 2 for s in top[1].split("┬")]
	if len(widths) != width or min(widths) < 3:
		raise Mismatch(f"top border has {len(widths)} columns for {width}: {text!r}")

	def rule(left, fill, join, right):
		return left + join.join(fill * (w + 2) for w in widths) + right

	def row(line):
		# every cell padded to exactly its column's width; nothing in this example ends in a space
		if not (line.startswith("│ ") and line.endswith(" │")):
			raise Mismatch(f"not a table row: {line!r}")
		parts = line[2:-2].split(" ┆ ")
		if len(parts) != width or any(len(c) != w for c, w in zip(parts, widths)):
			raise Mismatch(f"row {line!r} does not fit the borders {widths}")
		return [c.rstrip(" ") for c in parts]
	if len(lines) < 6 or lines[4] != rule("╞", "═", "╪", "╡") or lines[-1] != rule("└", "─", "┴", "┘"):
		raise Mismatch(f"unexpected borders: {text!r}")
	names, dashes, dtypes = row(lines[1]), row(lines[2]), row(lines[3])
	if dashes != ["---"] * width:
		raise Mismatch(f"no --- row: {dashes}")
	data = [row(line) for line in lines[5:-1]]
	if len(data) != (height if height <= ROWS_SHOWN else ROWS_SHOWN + 1):
		raise Mismatch(f"{len(data)} rows shown of {height}")
	if height > ROWS_SHOWN:
		if whole:
			raise Mismatch(f"a whole frame was required, but {height} rows are elided to {ROWS_SHOWN}")
		if data.pop(HALF) != ["…"] * width:
			raise Mismatch(f"no elided row after the first {HALF}: {text!r}")
	schema = []
	for name, dtype in zip(names, dtypes):
		if not re.fullmatch(r"[a-z_]+", name) or not re.fullmatch(r"[a-z0-9]+", dtype):
			raise Mismatch(f"unexpected schema entry {name!r}: {dtype!r}")
		schema.append((name, dtype))
	if len({n for n, _ in schema}) != width:
		raise Mismatch(f"duplicate column names: {schema}")
	cells = [[parse_cell(c, d, strings) for c, (_, d) in zip(r, schema)] for r in data]
	return height, width, schema, cells


def parse_cell(text, dtype, strings=SIMPLE):
	if dtype == "string":
		# unquoted (record 0158); these examples' strings are simple, so `…` is refused
		if not re.fullmatch(strings, text):
			raise Mismatch(f"not a simple string cell: {text!r}")
		return text
	if dtype == "f32":
		# record 0160: read back as the float32 its text rounds to, compared by value
		if not re.fullmatch(r"-?\d+\.\d+", text):
			raise Mismatch(f"not a finite float cell: {text!r}")
		return f32(float(text))
	if dtype in ("i64", "u32"):
		if not re.fullmatch(r"-?\d+", text):
			raise Mismatch(f"not an integer cell: {text!r}")
		return int(text)
	if dtype == "f64":
		if not re.fullmatch(r"-?\d+\.\d+", text):  # also refuses NaN and inf
			raise Mismatch(f"not a finite float cell: {text!r}")
		return float(text)
	if dtype == "bool":
		if text not in ("true", "false"):
			raise Mismatch(f"not a bool cell: {text!r}")
		return text == "true"
	raise Mismatch(f"dtype {dtype} is outside this example")


def same_rows(cells, expected, tolerance):
	"""Exact equality, except columns given a tolerance in `tolerance` ({index: tol})."""
	if not tolerance:
		return cells == expected
	return len(cells) == len(expected) and all(
		len(c) == len(e) and all(
			abs(a - b) <= tolerance[i] if i in tolerance else a == b for i, (a, b) in enumerate(zip(c, e)))
		for c, e in zip(cells, expected))


def check_frame(text, label, height, schema, rows, strings=SIMPLE, tolerance=None):
	"""Whole frames exactly; a taller one by its first and last five rows (record 0158)."""
	whole = height <= ROWS_SHOWN
	h, w, shown_schema, cells = parse_frame(text, whole, strings)
	if (h, w) != (height, len(schema)):
		raise Mismatch(f"{label}: {h} × {w}, expected {height} × {len(schema)}")
	if shown_schema != schema:
		raise Mismatch(f"{label}: schema {shown_schema}, expected {schema}")
	expected = rows if whole else rows[:HALF] + rows[-HALF:]
	if not same_rows(cells, expected, tolerance):
		raise Mismatch(f"{label}: rows differ:\n  shown    {cells}\n  expected {expected}")


def check_forms(text, form, label, height, schema, rows, strings=SIMPLE, tolerance=None):
	"""Record 0160 R1: one rnx value, two displayed forms. Both are parsed independently and must
	agree exactly (f32 cells by value, so an equal value in another valid spelling agrees); only
	that agreed frame is then compared with the expectation, under its tolerance."""
	whole = height <= ROWS_SHOWN
	if parse_frame(text, whole, strings) != parse_html_frame(form, whole, strings):
		raise Mismatch(f"{label}: text/plain and text/html show different frames")
	check_frame(text, label, height, schema, rows, strings, tolerance)


def results(nb):
	"""The text of each execute_result, in cell order."""
	texts = []
	for cell in nb.cells:
		for o in cell.get("outputs", []):
			if o["output_type"] == "execute_result":
				texts.append(o["data"]["text/plain"])
	return texts


def html_results(nb):
	"""Record 0159: each execute_result's HTML form, which must sit beside its text."""
	forms = []
	for cell in nb.cells:
		for o in cell.get("outputs", []):
			if o["output_type"] == "execute_result":
				if "text/plain" not in o["data"]:
					raise Mismatch("an HTML result without its text/plain fallback")
				forms.append(o["data"].get("text/html"))
	return forms


HTML_TAGS = ("div", "small", "table", "thead", "tbody", "tr", "th", "td")


def html_allowed(form):
	"""Record 0159: an independent copy of the allowlist rnx and the kernel apply: tags from a
	fixed list with no attributes, balanced, at most 8 deep; text with no raw < or >, & only in
	five forms, no control or bidi control except a newline; at most 16,384 bytes."""
	if len(form.encode()) > 16384:
		return False
	stack = []
	for part in re.split(r"(<[^<>]*>)", form):
		tag = re.fullmatch(r"<(/?)([a-z]+)>", part)
		if part.startswith("<"):
			if not tag or tag[2] not in HTML_TAGS:
				return False
			if tag[1]:
				if not stack or stack.pop() != tag[2]:
					return False
			elif len(stack) == 8:
				return False
			else:
				stack.append(tag[2])
			continue
		if "<" in part or ">" in part:
			return False
		if re.search(r"&(?!(amp|lt|gt|quot|#39);)", part):
			return False
		if any((unicode_control(c) and c != "\n") for c in part):
			return False
	return not stack


def unicode_control(c):
	return (ord(c) < 0x20 or 0x7f <= ord(c) < 0xa0 or c in "\u2028\u2029"
			or "\u202a" <= c <= "\u202e" or "\u2066" <= c <= "\u2069")


class Table(html.parser.HTMLParser):
	"""This example's HTML form: <div><small>shape</small><table><thead>two rows</thead>
	<tbody>rows</tbody></table></div>, cells as text with entities decoded."""

	def __init__(self):
		super().__init__(convert_charrefs=True)
		self.path, self.shape, self.head, self.body, self.cell = [], None, [], [], None

	def handle_starttag(self, tag, attrs):
		if attrs:
			raise Mismatch(f"an attribute on <{tag}>")
		self.path.append(tag)
		if tag == "tr":
			(self.head if "thead" in self.path else self.body).append([])
		if tag in ("th", "td"):
			self.cell = ""

	def handle_endtag(self, tag):
		if not self.path or self.path.pop() != tag:
			raise Mismatch(f"misnested </{tag}>")
		if tag in ("th", "td"):
			(self.head if "thead" in self.path else self.body)[-1].append(self.cell)
			self.cell = None

	def handle_data(self, data):
		if self.path and self.path[-1] == "small" and "table" not in self.path:
			self.shape = (self.shape or "") + data
		elif self.cell is not None:
			self.cell += data


def parse_html_frame(form, whole, strings=SIMPLE):
	"""The HTML twin of parse_frame: the same structural reading of elision."""
	if form is None or not html_allowed(form):
		raise Mismatch(f"no allowed HTML form: {form!r}")
	t = Table()
	t.feed(form)
	t.close()
	m = re.fullmatch(r"shape: \((\d[\d_]*), (\d[\d_]*)\)", t.shape or "")
	if not m:
		raise Mismatch(f"no HTML shape: {form!r}")
	if "[preview byte limit" in form:
		raise Mismatch("the HTML form hit its byte limit")
	height, width = int(m[1].replace("_", "")), int(m[2].replace("_", ""))
	if len(t.head) != 2 or any(len(r) != width for r in t.head + t.body):
		raise Mismatch(f"HTML rows do not have {width} cells: {form!r}")
	names, dtypes = t.head
	data = t.body
	if len(data) != (height if height <= ROWS_SHOWN else ROWS_SHOWN + 1):
		raise Mismatch(f"{len(data)} HTML rows shown of {height}")
	if height > ROWS_SHOWN:
		if whole:
			raise Mismatch(f"a whole frame was required, but {height} rows are elided")
		if data.pop(HALF) != ["…"] * width:
			raise Mismatch("no elided HTML row after the first five")
	schema = list(zip(names, dtypes))
	for name, dtype in schema:
		if not re.fullmatch(r"[a-z_]+", name) or not re.fullmatch(r"[a-z0-9]+", dtype):
			raise Mismatch(f"unexpected HTML schema entry {name!r}: {dtype!r}")
	cells = [[parse_cell(c, d, strings) for c, (_, d) in zip(r, schema)] for r in data]
	return height, width, schema, cells


def check_frame_html(form, label, height, schema, rows, strings=SIMPLE, tolerance=None):
	whole = height <= ROWS_SHOWN
	h, w, shown_schema, cells = parse_html_frame(form, whole, strings)
	if (h, w) != (height, len(schema)) or shown_schema != schema:
		raise Mismatch(f"{label} (HTML): {h} × {w} {shown_schema}, expected {height} × {len(schema)} {schema}")
	expected = rows if whole else rows[:HALF] + rows[-HALF:]
	if not same_rows(cells, expected, tolerance):
		raise Mismatch(f"{label} (HTML): rows differ:\n  shown    {cells}\n  expected {expected}")


def check_sales(nb, data):
	"""Every displayed frame, as text and as HTML, then the printed answer, against the CSV."""
	expected = sales_expected(data)
	forms = html_results(nb)  # first: it refuses an HTML form without its text fallback
	texts = results(nb)
	if len(texts) != len(expected["frames"]):
		raise Mismatch(f"{len(texts)} results shown, expected {len(expected['frames'])} frames")
	for text, form, (label, height, schema, rows) in zip(texts, forms, expected["frames"]):
		check_forms(text, form, label, height, schema, rows)
	check_sales_answer(judge(nb), expected)


def check_sales_answer(stdout, expected):
	m = re.fullmatch(r"Net revenue: \$(\d+)\.(\d\d) from (\d+) orders \((\d+) returned, left out\)\n"
					 r"Top region: ([a-z]+), \$(\d+)\.(\d\d), (\d+)\.(\d)% of net revenue\n", stdout)
	if not m:
		raise Mismatch(f"unexpected answer lines {stdout!r}")
	shown = (int(m[1]) * 100 + int(m[2]), int(m[3]), int(m[4]), m[5], int(m[6]) * 100 + int(m[7]),
			 int(m[8]) * 10 + int(m[9]))
	if shown != expected["answer"]:
		raise Mismatch(f"answer {shown}, expected {expected['answer']}")


# ---- 05/06: the Candle notebooks (plans/0160 and its section 5a) -------------------------

CANDLE_NOTEBOOKS = {"05_candle_model": False, "06_candle_search": True}  # name: needs the model
MODEL_DIR = DEMOS / "models" / "all-MiniLM-L6-v2"
MODEL_FILES = {
	"config.json": "953f9c0d463486b10a6871cc2fd59f223b2c70184f49815e7efbcab5d8908b41",
	"tokenizer.json": "be50c3628f2bf5bb5e3a7f17b1f74611b2561a3a27eeab05e5aa30f411572037",
	"model.safetensors": "53aa51172d142c89d9012cce15ae4d6cc0ca6895895114379cacb4fab128d9db",
	"modules.json": "84e40c8e006c9b1d6c122e02cba9b02458120b5fb0c87b746c41e0207cf642cf",
	"1_Pooling/config.json": "4be450dde3b0273bb9787637cfbd28fe04a7ba6ab9d36ac48e92b11e350ffc23",
	"sentence_bert_config.json": "fc1993fde0a95c24ec6c022539d41cf6e2f7c9721e5415d6fb6897472a9cd4b7",
}
REVISION = "1110a243fdf4706b3f48f1d95db1a4f5529b4d41"
# the bundled 05 assets: their bytes are pinned (0129's writer is not claimed byte-reproducible)
ASSETS = {
	"features.csv": "cf586d5a5577e5c2406dd15aa28ae6fbe9bbb336b581e934a26d06e1e05ebe38",
	"mlp.safetensors": "5d9a064f78af2f3ebe126a30dfd5ce488d850b84a59a8669874c0c045e4d5bfb",
}
SEARCH_TOLERANCE = 1e-5
SHOWN = 3
GAP = 1e-4
SETUP = "run sh demos/candle/fetch-model.sh from the checkout root"


def sha256(path):
	return hashlib.sha256(pathlib.Path(path).read_bytes()).hexdigest()


def model_ready(model_dir=MODEL_DIR):
	"""Verification, distinct from acquisition: the six pinned files, by name, before 06 runs."""
	for name, want in MODEL_FILES.items():
		path = pathlib.Path(model_dir) / name
		if not path.is_file():
			raise Mismatch(f"model file {name} is missing from {model_dir}: {SETUP}")
		if sha256(path) != want:
			raise Mismatch(f"model file {name} differs from the pinned revision: {SETUP}")


def candle_terminal(worker, name, data=DEMOS / "data", model_dir=MODEL_DIR):
	"""The worker runs the notebook's terminal script with absolute paths, from the checkout root."""
	script = {"05_candle_model": "model.rn", "06_candle_search": "search.rn"}[name]
	args = [str(data)] + ([str(model_dir)] if CANDLE_NOTEBOOKS[name] else [])
	r = subprocess.run([worker, "run", str(DEMOS / "candle" / script), *args], cwd=DEMOS.parent,
					   capture_output=True, text=True)
	if r.returncode != 0 or r.stderr:
		raise Mismatch(f"{script} failed: exit {r.returncode}, stderr {r.stderr!r}")
	return r.stdout


def read_safetensors(path):
	"""The MLP's four F32 tensors, by a reader of our own: names, shapes, dtype and offsets checked."""
	raw = pathlib.Path(path).read_bytes()
	if len(raw) < 8:
		raise Mismatch("mlp.safetensors: too short")
	n = struct.unpack("<Q", raw[:8])[0]
	header = json.loads(raw[8:8 + n])
	header.pop("__metadata__", None)
	want = {"fc1.weight": [8, 3], "fc1.bias": [8], "fc2.weight": [1, 8], "fc2.bias": [1]}
	if sorted(header) != sorted(want):
		raise Mismatch(f"mlp.safetensors: tensors {sorted(header)}")
	body = raw[8 + n:]
	out = {}
	for name, shape in want.items():
		h = header[name]
		count = math.prod(shape)
		start, end = h["data_offsets"]
		if h["dtype"] != "F32" or h["shape"] != shape or end - start != 4 * count or end > len(body):
			raise Mismatch(f"mlp.safetensors: {name} is {h}")
		out[name] = list(struct.unpack(f"<{count}f", body[start:end]))
	return out


def model_expected(data=DEMOS / "data", pinned=True):
	"""05's every shown value, recomputed: the features from the CSV, the MLP's forward pass in
	Python from the weights it read itself. Exactness is established for these fixed assets only:
	every product and every partial sum, in any order, is a float32 exactly, so GEMM order and
	fused multiply-add can't change a result. It is not a claim about arbitrary models."""
	for name, want in ASSETS.items():
		if pinned and sha256(pathlib.Path(data) / name) != want:
			raise Mismatch(f"{name} differs from the bundled asset")
	with open(pathlib.Path(data) / "features.csv", newline="") as f:
		r = csv.reader(f)
		if next(r) != ["id", "a", "b", "c"]:
			raise Mismatch("features.csv: unexpected header")
		rows = [[int(i), float(a), float(b), float(c)] for i, a, b, c in r]
	w = read_safetensors(pathlib.Path(data) / "mlp.safetensors")

	def exact_sum(terms):
		# every subset sum of the terms is a float32 exactly: any order gives the same result
		for k in range(1, len(terms) + 1):
			for subset in itertools.combinations(terms, k):
				s = math.fsum(subset)
				if f32(s) != s:
					raise Mismatch("the MLP fixture is not exact in float32; the exact check doesn't apply")
		return math.fsum(terms)
	scores = []
	for _, a, b, c in rows:
		x = [f32(a), f32(b), f32(c)]
		hidden = []
		for j in range(8):
			terms = [w["fc1.weight"][3 * j + k] * x[k] for k in range(3)] + [w["fc1.bias"][j]]
			hidden.append(max(0.0, exact_sum(terms)))
		y = exact_sum([w["fc2.weight"][j] * hidden[j] for j in range(8)] + [w["fc2.bias"][0]])
		scores.append(y)
	scored = [r + [s] for r, s in zip(rows, scores)]
	order = sorted(range(len(scored)), key=lambda i: -scores[i])
	if len({scores[i] for i in order[:6]}) != 6:
		raise Mismatch("tied scores at the shown top-five boundary")
	features = [("id", "i64"), ("a", "f64"), ("b", "f64"), ("c", "f64")]
	return {
		"frames": {
			0: ("features", len(rows), features, rows),
			4: ("scored", len(rows), features + [("score", "f32")], scored),
			5: ("top", 5, features + [("score", "f32")], [scored[i] for i in order[:5]]),
		},
		"tensors": {1: ((64, 3), [[f32(v) for v in r[1:]] for r in rows]), 3: ((64, 1), [[s] for s in scores])},
		"texts": {2: "(3, 8, 1)"},
		"answer": (order[0], scores[order[0]], len(rows)),
	}


def parse_tensor(text):
	"""This example's tensor display only: `Tensor[f32; RxC]`, then up to 8 rows of ` | `-separated
	values, then `…` when rows were left out."""
	lines = text.split("\n")
	m = re.fullmatch(r"Tensor\[f32; (\d+)x(\d+)\]", lines[0])
	if not m:
		raise Mismatch(f"not an f32 matrix display: {text!r}")
	shape = (int(m[1]), int(m[2]))
	body = [l for l in lines[1:] if l != ""]
	more = body and body[-1] == "…"
	if more:
		body.pop()
	if more != (shape[0] > 8) or len(body) != min(shape[0], 8) or shape[1] > 8:
		raise Mismatch(f"tensor display does not match its shape {shape}: {text!r}")
	values = []
	for line in body:
		cells = line.split(" | ")
		if len(cells) != shape[1] or not all(re.fullmatch(r"-?\d+\.\d+", c) for c in cells):
			raise Mismatch(f"tensor row {line!r}")
		values.append([f32(float(c)) for c in cells])
	return shape, values


def parse_tensor_html(form):
	"""Record 0161: this example's tensor HTML only, read independently of the text: the header in
	<small>, column indices 0..n in the head, a row index 0..m in each row, f32 values, and a final
	`…` row when rows were left out. Returns what parse_tensor returns."""
	if form is None or not html_allowed(form):
		raise Mismatch(f"no allowed tensor HTML: {form!r}")
	t = Table()
	t.feed(form)
	t.close()
	m = re.fullmatch(r"Tensor\[f32; (\d+)x(\d+)\]", t.shape or "")
	if not m:
		raise Mismatch(f"not an f32 matrix header: {t.shape!r}")
	shape = (int(m[1]), int(m[2]))
	if len(t.head) != 1:
		raise Mismatch(f"tensor HTML head rows: {t.head}")
	width = min(shape[1], 8)
	if shape[1] > 8 or t.head[0] != [""] + [str(i) for i in range(width)]:
		raise Mismatch(f"tensor HTML column indices: {t.head[0]}")
	body = t.body
	more = shape[0] > 8
	if len(body) != min(shape[0], 8) + more:
		raise Mismatch(f"tensor HTML has {len(body)} rows for shape {shape}")
	if more and body.pop() != ["…"] * (width + 1):
		raise Mismatch("tensor HTML has no elided row where rows were left out")
	values = []
	for i, row in enumerate(body):
		if len(row) != width + 1 or row[0] != str(i) or not all(re.fullmatch(r"-?\d+\.\d+", c) for c in row[1:]):
			raise Mismatch(f"tensor HTML row {i}: {row}")
		values.append([f32(float(c)) for c in row[1:]])
	return shape, values


def check_model(nb, data=DEMOS / "data", pinned=True):
	"""Every shown result of 05, in order and exactly once, then its answer line."""
	expected = model_expected(data, pinned)
	forms = html_results(nb)
	texts = results(nb)
	if len(texts) != 6:
		raise Mismatch(f"{len(texts)} results shown, expected 6")
	for i, text in enumerate(texts):
		if i in expected["frames"]:
			label, height, schema, rows = expected["frames"][i]
			check_forms(text, forms[i], label, height, schema, rows)
		elif i in expected["tensors"]:
			# record 0161: both forms parsed independently, agreeing exactly before the expectation
			shape, values = parse_tensor(text)
			if parse_tensor_html(forms[i]) != (shape, values):
				raise Mismatch(f"result {i}: the tensor's text/plain and text/html differ")
			want_shape, want = expected["tensors"][i]
			if shape != want_shape or values != want[:8]:
				raise Mismatch(f"result {i}: tensor {shape} {values}, expected {want_shape} {want[:8]}")
		elif text != expected["texts"][i]:
			raise Mismatch(f"result {i}: {text!r}, expected {expected['texts'][i]!r}")
	check_model_answer(judge(nb), expected)


def check_model_answer(stdout, expected):
	m = re.fullmatch(r"Highest score: row (\d+) with (-?\d+\.\d+), of (\d+) rows scored\n", stdout)
	if not m or (int(m[1]), f32(float(m[2])), int(m[3])) != expected["answer"]:
		raise Mismatch(f"answer {stdout!r}, expected {expected['answer']}")


def search_reference(path=DEMOS / "data" / "search-reference.json", data=DEMOS / "data"):
	"""The committed sentence-transformers reference, validated before its outputs are used:
	provenance (the exact CSV bytes, the six model files, the revision), then the rankings
	themselves (unique identities, complete, finite, ordered, ties by id, a stable top 3)."""
	ref = json.loads(pathlib.Path(path).read_text())
	for name in ("articles.csv", "questions.csv"):
		if ref["inputs"].get(name) != sha256(pathlib.Path(data) / name):
			raise Mismatch(f"search-reference.json was made from a different {name}")
	if ref["model"]["files"] != MODEL_FILES or ref["model"]["revision"] != REVISION:
		raise Mismatch("search-reference.json was made from a different model")
	articles = rows_of(pathlib.Path(data) / "articles.csv", ["id", "title", "text"])
	questions = rows_of(pathlib.Path(data) / "questions.csv", ["id", "question"])
	ids = [int(a[0]) for a in articles]
	if ids != list(range(len(ids))) or [int(q[0]) for q in questions] != list(range(len(questions))):
		raise Mismatch("article or question ids are not 0..n in order")
	rankings = ref["rankings"]
	if [r["question"] for r in rankings] != list(range(len(questions))):
		raise Mismatch("the reference does not rank every question exactly once")
	for r in rankings:
		ranked = r["ranking"]
		if sorted(x["article"] for x in ranked) != ids:
			raise Mismatch(f"question {r['question']}: the ranking is not every article exactly once")
		scores = [x["score"] for x in ranked]
		if not all(isinstance(s, float) and math.isfinite(s) for s in scores):
			raise Mismatch(f"question {r['question']}: a non-finite score")
		keys = [(-x["score"], x["article"]) for x in ranked]
		if keys != sorted(keys):
			raise Mismatch(f"question {r['question']}: the ranking is not sorted by score, then id")
		if min(scores[k] - scores[k + 1] for k in range(SHOWN)) <= GAP:
			raise Mismatch(f"question {r['question']}: ranks 1-4 are not separated by more than {GAP}")
	return ref, articles, questions


def rows_of(path, header):
	with open(path, newline="") as f:
		r = csv.reader(f)
		if next(r) != header:
			raise Mismatch(f"{path.name}: unexpected header")
		return list(r)


def search_expected(reference=DEMOS / "data" / "search-reference.json", data=DEMOS / "data"):
	ref, articles, questions = search_reference(reference, data)
	title = {int(a[0]): a[1] for a in articles}
	top = [[[x["article"], title[x["article"]], x["score"]] for x in r["ranking"][:SHOWN]] for r in ref["rankings"]]
	frames = {0: ("articles", len(articles), [("id", "i64"), ("title", "string")],
				  [[int(a[0]), a[1]] for a in articles])}
	for i, rows in enumerate(top):
		frames[3 + i] = (f"question {i}", SHOWN, [("id", "i64"), ("title", "string"), ("score", "f32")], rows)
	asked = [q[1] for q in questions]
	stdout = "".join(f"{q}\n" for q in asked) + "".join(f"{q} -> {t[0][1]}\n" for q, t in zip(asked, top))
	return {"frames": frames,
			"texts": {1: "TextEncoder[bert; 6 layers, 384 dims, max 256 tokens]",
					  2: f"Dense[f32; {len(articles)} x 384](e0, e1, e2, e3, e4, e5, e6, e7, …)"},
			"stdout": stdout}


def check_search(nb, reference=DEMOS / "data" / "search-reference.json", data=DEMOS / "data"):
	"""Every shown result of 06 against the reference: identities and order exactly, scores within
	SEARCH_TOLERANCE; then the printed questions and best articles."""
	expected = search_expected(reference, data)
	forms = html_results(nb)
	texts = results(nb)
	if len(texts) != 3 + 6:
		raise Mismatch(f"{len(texts)} results shown, expected 9")
	for i, text in enumerate(texts):
		if i in expected["frames"]:
			label, height, schema, rows = expected["frames"][i]
			tolerance = {2: SEARCH_TOLERANCE} if i >= 3 else None
			check_forms(text, forms[i], label, height, schema, rows, TITLES, tolerance)
		else:
			if forms[i] is not None or text != expected["texts"][i]:
				raise Mismatch(f"result {i}: {text!r}, expected {expected['texts'][i]!r}")
	if judge(nb) != expected["stdout"]:
		raise Mismatch(f"printed lines differ:\n{judge(nb)!r}\n{expected['stdout']!r}")


def check_candle(name, nb):
	(check_model if name == "05_candle_model" else check_search)(nb)


def generate(kernel_name, name):
	if CANDLE_NOTEBOOKS.get(name):
		model_ready()
	path = HERE / f"{name}.ipynb"
	nb = nbformat.read(path, as_version=4)
	ran = execute(nb, kernel_name)[0]
	judge(ran)
	for cell in ran.cells:
		cell.metadata = {}
		for o in cell.get("outputs", []):
			o.pop("metadata", None) if o.get("output_type") == "stream" else o.update(metadata={})
	ran.metadata = nb.metadata
	nbformat.write(ran, path)


class CandleControls:
	"""Record 0160's controls (plans/0160 section 5a, item 5), mixed into Controls."""

	def candle(self, worker, kernel_name):
		model = execute(nbformat.read(HERE / "05_candle_model.ipynb", as_version=4), kernel_name)[0]
		search = execute(nbformat.read(HERE / "06_candle_search.ipynb", as_version=4), kernel_name)[0]
		self.expect("05 unaltered against the recomputed model", lambda: check_model(model), True)
		self.expect("06 unaltered against the reference", lambda: check_search(search), True)

		def result_text(nb, i, fn):
			nb = copy.deepcopy(nb)
			outs = [o for c in nb.cells for o in c.get("outputs", []) if o["output_type"] == "execute_result"]
			outs[i]["data"]["text/plain"] = fn(outs[i]["data"]["text/plain"])
			return nb

		def result_html(nb, i, fn):
			nb = copy.deepcopy(nb)
			outs = [o for c in nb.cells for o in c.get("outputs", []) if o["output_type"] == "execute_result"]
			outs[i]["data"]["text/html"] = fn(outs[i]["data"]["text/html"])
			return nb

		# 05: a weight that changes the checked result, in a temporary copy (fc2.bias 0.5 -> 0.75)
		with tempfile.TemporaryDirectory(prefix="rnx-0160-mlp-") as d:
			for name in ("features.csv", "mlp.safetensors"):
				shutil.copy2(DEMOS / "data" / name, d)
			raw = bytearray((pathlib.Path(d) / "mlp.safetensors").read_bytes())
			n = struct.unpack("<Q", raw[:8])[0]
			start = 8 + n + json.loads(raw[8:8 + n])["fc2.bias"]["data_offsets"][0]
			assert struct.unpack("<f", raw[start:start + 4])[0] == 0.5
			raw[start:start + 4] = struct.pack("<f", 0.75)
			(pathlib.Path(d) / "mlp.safetensors").write_bytes(bytes(raw))
			assert model_expected(d, pinned=False)["answer"] != model_expected()["answer"]
			self.expect("05 against a changed fc2.bias", lambda: check_model(model, d, pinned=False), False)
			self.expect("the changed weights refused by their pinned hash", lambda: model_expected(pathlib.Path(d)), False)
		self.expect("a shown score changed (05)",
					lambda: check_model(result_text(model, 4, lambda s: s.replace("0.765625  ", "0.765626  ", 1))), False)
		self.expect("a tensor shape changed (05)",
					lambda: check_model(result_text(model, 1, lambda s: s.replace("Tensor[f32; 64x3]", "Tensor[f32; 65x3]"))), False)
		# record 0161: the tensor's HTML form
		def tensor_html(fn):
			return result_html(model, 1, fn)
		x_first = "<th>0</th><td>0.0</td><td>-3.0</td><td>0.0</td>"
		nudged = repr(struct.unpack("<f", struct.pack("<I", struct.unpack("<I", struct.pack("<f", -3.0))[0] + 1))[0])
		self.expect("an HTML tensor value changed by one f32 step (05)",
					lambda: check_model(tensor_html(lambda h: h.replace(x_first, x_first.replace("-3.0", nudged), 1))), False)
		self.expect("the same tensor value in another valid spelling in HTML (05)",
					lambda: check_model(tensor_html(lambda h: h.replace(x_first, x_first.replace("-3.0", "-3.00"), 1))), True)
		self.expect("an HTML tensor row index changed (05)",
					lambda: check_model(tensor_html(lambda h: h.replace("<tr><th>1</th>", "<tr><th>9</th>", 1))), False)
		self.expect("an HTML tensor column index changed (05)",
					lambda: check_model(tensor_html(lambda h: h.replace("<th>2</th></tr></thead>", "<th>3</th></tr></thead>", 1))), False)
		self.expect("the HTML tensor's elided row removed (05)",
					lambda: check_model(tensor_html(lambda h: re.sub(r"<tr><th>…</th>(<td>…</td>)+</tr>", "", h))), False)
		self.expect("an HTML tensor row duplicated (05)",
					lambda: check_model(tensor_html(lambda h: h.replace("<tr><th>1</th>", "<tr><th>0</th><td>0.0</td><td>-3.0</td><td>0.0</td></tr><tr><th>1</th>", 1))), False)
		self.expect("an HTML tensor value missing (05)",
					lambda: check_model(tensor_html(lambda h: h.replace(x_first, "<th>0</th><td>0.0</td><td>-3.0</td>", 1))), False)
		self.expect("the HTML tensor header shape changed (05)",
					lambda: check_model(tensor_html(lambda h: h.replace("Tensor[f32; 64x3]", "Tensor[f32; 65x3]", 1))), False)
		self.expect("an attribute injected into the tensor HTML (05)",
					lambda: check_model(tensor_html(lambda h: h.replace("<table>", "<table onclick=x>", 1))), False)

		def tensor_without_text(nb):
			nb = copy.deepcopy(nb)
			outs = [o for c in nb.cells for o in c.get("outputs", []) if o["output_type"] == "execute_result"]
			del outs[1]["data"]["text/plain"]
			return nb
		self.expect("a tensor's text/html without its text/plain (05)", lambda: check_model(tensor_without_text(model)), False)
		self.expect("a tensor value changed (05)",
					lambda: check_model(result_text(model, 3, lambda s: s.replace("0.640625", "0.640626", 1))), False)

		# 06: rows and identities
		def swap_rows(text):
			lines = text.split("\n")
			lines[6], lines[7] = lines[7], lines[6]
			return "\n".join(lines)
		self.expect("two ranked rows swapped (06)", lambda: check_search(result_text(search, 3, swap_rows)), False)
		self.expect("a ranked row missing (06)",
					lambda: check_search(result_text(search, 3, lambda s: "\n".join(l for k, l in enumerate(s.split("\n")) if k != 7))), False)
		self.expect("a ranked row duplicated (06)",
					lambda: check_search(result_text(search, 3, lambda s: "\n".join(s.split("\n")[:7] + s.split("\n")[6:]))), False)
		# R1: the two forms of one value must agree exactly, inside the reference tolerance too
		first = parse_frame(results(search)[3], True, TITLES)[3][0][2]
		bits = struct.unpack("<I", struct.pack("<f", first))[0]
		neighbour = struct.unpack("<f", struct.pack("<I", bits + 1))[0]
		assert abs(neighbour - first) < SEARCH_TOLERANCE and neighbour != first
		spelled = re.search(r"0\.\d+", results(search)[3].split("\n")[6])[0]
		# the neighbour's shortest spelling that reads back to it, as rnx would write it
		short = next(s for s in (f"{neighbour:.{p}g}" for p in range(1, 12)) if f32(float(s)) == neighbour)
		assert short != spelled

		def text_score(s):
			lines = s.split("\n")
			cells = lines[6][2:-2].split(" ┆ ")
			assert len(short) <= len(cells[2])
			cells[2] = short.ljust(len(cells[2]))  # the same width: only the value differs
			lines[6] = "│ " + " ┆ ".join(cells) + " │"
			return "\n".join(lines)
		self.expect("an HTML-only score change inside the reference tolerance (06)",
					lambda: check_search(result_html(search, 3, lambda h: h.replace(f"<td>{spelled}</td>", f"<td>{short}</td>", 1))), False)
		self.expect("a text-only score change inside the reference tolerance (06)",
					lambda: check_search(result_text(search, 3, text_score)), False)
		self.expect("the same score in another valid spelling in HTML (06)",
					lambda: check_search(result_html(search, 3, lambda h: h.replace(f"<td>{spelled}</td>", f"<td>{spelled}0</td>", 1))), True)
		self.expect("an HTML ranked title changed (06)",
					lambda: check_search(result_html(search, 3, lambda h: h.replace("<td>Reset your password</td>", "<td>Change your email address</td>", 1))), False)

		# 06: the reference itself
		ref = json.loads((DEMOS / "data" / "search-reference.json").read_text())
		shown = parse_frame(results(search)[3], True, TITLES)[3][0][2]
		with tempfile.TemporaryDirectory(prefix="rnx-0160-ref-") as d:
			shifted = copy.deepcopy(ref)
			shifted["rankings"][0]["ranking"][0]["score"] += 2e-5
			assert abs(shown - shifted["rankings"][0]["ranking"][0]["score"]) > SEARCH_TOLERANCE
			path = pathlib.Path(d) / "shifted.json"
			path.write_text(json.dumps(shifted))
			self.expect("a shown reference score shifted by 2e-5", lambda: check_search(search, path), False)
			moved = copy.deepcopy(ref)
			moved["model"]["revision"] = "0" * 40
			path = pathlib.Path(d) / "revision.json"
			path.write_text(json.dumps(moved))
			self.expect("a reference made from another model revision", lambda: check_search(search, path), False)
			dup = copy.deepcopy(ref)
			dup["rankings"][1]["ranking"][5] = dup["rankings"][1]["ranking"][4]
			path = pathlib.Path(d) / "duplicate.json"
			path.write_text(json.dumps(dup))
			self.expect("a reference ranking with a duplicate article", lambda: check_search(search, path), False)
			data = pathlib.Path(d) / "data"
			data.mkdir()
			for name in ("articles.csv", "questions.csv"):
				shutil.copy2(DEMOS / "data" / name, data)
			text = (data / "questions.csv").read_text()
			(data / "questions.csv").write_text(text.replace("plane", "train", 1))
			self.expect("questions.csv changed after the reference was made",
						lambda: check_search(search, DEMOS / "data" / "search-reference.json", data), False)
			empty = pathlib.Path(d) / "no-model"
			empty.mkdir()

			def missing():
				try:
					model_ready(empty)
				except Mismatch as e:
					if "fetch-model.sh" not in str(e):
						raise AssertionError(f"no setup command in {e}")
					raise
			self.expect("the model directory missing (names the setup command)", missing, False)


class Controls(CandleControls):
	"""Each control must give its stated verdict (plans/0156 5a, items 2 and 3; plans/0157 5a)."""

	def __init__(self):
		self.ok = True

	def expect(self, name, fn, should_pass):
		try:
			fn()
			passed, detail = True, "accepted"
		except Mismatch as e:
			passed, detail = False, str(e).splitlines()[0]
		good = passed == should_pass
		self.ok &= good
		print(f"{'pass' if good else 'WRONG'}: {name}: {detail}")

	def common(self, stored, ran, kernel_name, answer):
		"""The 0156 controls on one notebook: `answer` is (old, new) text inside its first
		stdout record."""
		printing = next(i for i, c in enumerate(ran.cells) if any(o.get("name") == "stdout" for o in c.get("outputs", [])))

		def split(nb):
			nb = copy.deepcopy(nb)
			cell = nb.cells[printing]
			text = cell.outputs[0]["text"]
			cut = len(text) // 2
			cell.outputs[0:1] = [nbformat.v4.new_output("stream", name="stdout", text=text[:cut]),
								 nbformat.v4.new_output("stream", name="stdout", text=text[cut:])]
			return nb
		self.expect("a harmless same-stream split", lambda: compare(stored, split(ran)), True)

		def changed(nb):
			nb = copy.deepcopy(nb)
			o = nb.cells[printing].outputs[0]
			o["text"] = o["text"].replace(answer[0], answer[1], 1)
			assert o["text"] != ran.cells[printing].outputs[0]["text"]
			return nb
		self.expect("one byte of an answer changed", lambda: compare(stored, changed(ran)), False)

		shown = [i for i, c in enumerate(ran.cells) if any(o["output_type"] == "execute_result" for o in c.get("outputs", []))]
		a, b = shown[0], shown[1]
		assert outputs(ran.cells[a]) != outputs(ran.cells[b])

		def swapped(nb):
			nb = copy.deepcopy(nb)
			nb.cells[a].outputs, nb.cells[b].outputs = nb.cells[b].outputs, nb.cells[a].outputs
			return nb
		self.expect("two different results swapped between cells", lambda: compare(stored, swapped(ran)), False)

		def stderr(nb):
			nb = copy.deepcopy(nb)
			nb.cells[printing].outputs[0]["name"] = "stderr"
			return nb
		self.expect("stdout turned into stderr (comparison)", lambda: compare(stored, stderr(ran)), False)
		self.expect("stdout turned into stderr (judge)", lambda: judge(stderr(ran)), False)

		broken = copy.deepcopy(stored)
		broken.cells.append(nbformat.v4.new_code_cell("let x = [1, 2];\nx[5]"))
		self.expect("a cell that raises", lambda: judge(execute(broken, kernel_name)[0]), False)

	def plain(self, rnx, kernel_name):
		stored = nbformat.read(HERE / "02_orders.ipynb", as_version=4)
		ran = execute(stored, kernel_name)[0]
		self.common(stored, ran, kernel_name, ("245.00", "245.01"))

		def wrong_line():
			reference = terminal(rnx, "02_orders")
			if judge(ran) != reference.replace("Still open: 4", "Still open: 5"):
				raise Mismatch("the stream no longer matches the terminal reference")
		self.expect("a stream line not matching the terminal reference", wrong_line, False)

	def polars(self, worker, kernel_name):
		stored = nbformat.read(HERE / f"{POLARS_NOTEBOOK}.ipynb", as_version=4)
		ran = execute(stored, kernel_name)[0]
		self.expect("the unaltered run against sales.csv", lambda: check_sales(ran, SALES), True)
		self.common(stored, ran, kernel_name, ("1227.50", "1227.51"))

		def wrong_line():
			reference = polars_terminal(worker, SALES)
			if judge(ran) != reference.replace("from 20 orders", "from 21 orders"):
				raise Mismatch("the stream no longer matches the terminal reference")
		self.expect("a stream line not matching the terminal reference", wrong_line, False)

		# a CSV value edited in a temporary copy: the first north order's units 3 -> 4
		with tempfile.TemporaryDirectory(prefix="rnx-0157-csv-") as d:
			edited = pathlib.Path(d) / "sales.csv"
			text = SALES.read_text()
			row = "2026-07,north,widget,3,1250,false\n"
			assert text.count(row) == 1
			edited.write_text(text.replace(row, row.replace(",3,", ",4,")))
			before = sales_expected(SALES)["frames"][2][3]
			after = sales_expected(edited)["frames"][2][3]
			assert before != after, "the edit must change a checked region result"
			self.expect("the stored outputs against an edited CSV", lambda: check_sales(ran, edited), False)
			self.expect("the edited CSV's own terminal answer against the original's expectations",
						lambda: check_sales_answer(polars_terminal(worker, edited), sales_expected(SALES)), False)

		index = next(i for i, c in enumerate(ran.cells) if any(
			o["output_type"] == "execute_result" and '┆ orders' in o["data"]["text/plain"]
			for o in c.get("outputs", [])))

		def edited_region(fn):
			nb = copy.deepcopy(ran)
			o = next(o for o in nb.cells[index].outputs if o["output_type"] == "execute_result")
			o["data"]["text/plain"] = fn(o["data"]["text/plain"])
			return nb

		# lines: shape, top border, names, ---, dtypes, separator, then the data rows
		def swap_rows(text):
			lines = text.split("\n")
			lines[6], lines[7] = lines[7], lines[6]
			return "\n".join(lines)
		self.expect("two rows swapped in the region frame", lambda: check_sales(edited_region(swap_rows), SALES), False)

		def elision_row(line):
			return "│ " + " ┆ ".join("…".ljust(len(c)) for c in line[2:-2].split(" ┆ ")) + " │"

		def elided(text):
			lines = text.split("\n")
			return "\n".join(lines[:7] + [elision_row(lines[6])] + lines[7:])
		self.expect("an elided row where the region frame must be whole",
					lambda: check_sales(edited_region(elided), SALES), False)

		def ellipsis_cell(text):
			lines = text.split("\n")
			cells = lines[6][2:-2].split(" ┆ ")
			cells[0] = "…".ljust(len(cells[0]))
			lines[6] = "│ " + " ┆ ".join(cells) + " │"
			return "\n".join(lines)
		self.expect("a literal … cell where no row is elided",
					lambda: check_sales(edited_region(ellipsis_cell), SALES), False)

		def duplicated(text):
			lines = text.split("\n")
			return "\n".join(lines[:7] + lines[6:7] + lines[7:])
		self.expect("a duplicated row in the region frame", lambda: check_sales(edited_region(duplicated), SALES), False)

		def non_finite(nb):
			nb = copy.deepcopy(nb)
			for c in nb.cells:
				for o in c.get("outputs", []):
					if o["output_type"] == "execute_result" and 'mean_cents' in o["data"]["text/plain"]:
						o["data"]["text/plain"] = o["data"]["text/plain"].replace("6137.5", "NaN   ")
			return nb
		self.expect("a non-finite mean", lambda: check_sales(non_finite(ran), SALES), False)

		# record 0159: the HTML form
		def edited_region_html(fn):
			nb = copy.deepcopy(ran)
			o = next(o for o in nb.cells[index].outputs if o["output_type"] == "execute_result")
			o["data"]["text/html"] = fn(o["data"]["text/html"])
			return nb
		self.expect("one HTML cell changed",
					lambda: check_sales(edited_region_html(lambda h: h.replace("<td>38750</td>", "<td>38751</td>", 1)), SALES), False)

		def swap_html_rows(h):
			rows = re.findall(r"<tr>.*?</tr>", h)
			return h.replace(rows[2], "\0").replace(rows[3], rows[2]).replace("\0", rows[3])
		self.expect("two HTML rows swapped", lambda: check_sales(edited_region_html(swap_html_rows), SALES), False)
		self.expect("an injected <script>",
					lambda: check_sales(edited_region_html(lambda h: h.replace("<td>north</td>", "<td><script>alert(1)</script></td>", 1)), SALES), False)
		self.expect("an injected onerror attribute",
					lambda: check_sales(edited_region_html(lambda h: h.replace("<td>north</td>", "<td onerror=alert(1)>north</td>", 1)), SALES), False)
		self.expect("an HTML row missing",
					lambda: check_sales(edited_region_html(lambda h: h.replace(re.findall(r"<tr>.*?</tr>", h)[3], "", 1)), SALES), False)

		def no_text(nb):
			nb = copy.deepcopy(nb)
			o = next(o for o in nb.cells[index].outputs if o["output_type"] == "execute_result")
			del o["data"]["text/plain"]
			return nb
		self.expect("text/html without its text/plain fallback", lambda: check_sales(no_text(ran), SALES), False)


def main():
	p = argparse.ArgumentParser()
	p.add_argument("--rnx", required=True)
	p.add_argument("--kernel", required=True)
	p.add_argument("--polars", help="the Polars worker built from demos/polars/")
	p.add_argument("--candle", help="the Candle worker built from demos/candle/ (a superset: runs every notebook)")
	p.add_argument("--only", help="comma-separated notebook names, e.g. 05_candle_model to skip the model download")
	mode = p.add_mutually_exclusive_group()
	mode.add_argument("--generate", action="store_true")
	mode.add_argument("--controls", action="store_true")
	a = p.parse_args()
	for exe in (a.rnx, a.kernel) + tuple(x for x in (a.polars, a.candle) if x):
		if not os.path.isabs(exe) or not os.access(exe, os.X_OK):
			sys.exit(f"{exe}: give an absolute path to an executable")
	workers = {"rnx-check": a.rnx}
	if a.polars:
		workers["rnx-polars-check"] = a.polars
	if a.candle:
		workers["rnx-candle-check"] = a.candle
	kernelspecs(workers, a.kernel)
	# (notebook, terminal executable, kernel, restart): the plain set on plain rnx; with --polars
	# the Polars notebook and the plain set on the Polars worker; with --candle the Candle notebooks
	# and every other notebook on the Candle worker
	plan = [(name, a.rnx, "rnx-check", name == "02_orders") for name in NOTEBOOKS]
	if a.polars:
		plan += [(POLARS_NOTEBOOK, a.polars, "rnx-polars-check", True)]
		plan += [(name, a.polars, "rnx-polars-check", False) for name in NOTEBOOKS]
	if a.candle:
		plan += [(name, a.candle, "rnx-candle-check", name == "05_candle_model") for name in CANDLE_NOTEBOOKS]
		plan += [(name, a.candle, "rnx-candle-check", False) for name in [*NOTEBOOKS, POLARS_NOTEBOOK]]
	if a.only:
		only = a.only.split(",")
		unknown = set(only) - {n for n, *_ in plan}
		if unknown:
			sys.exit(f"--only: not in this run: {sorted(unknown)}")
		plan = [step for step in plan if step[0] in only]
	if a.generate:
		# each notebook is written by the worker it belongs to; the others are left alone
		if a.candle:
			targets = [(n, "rnx-candle-check") for n in CANDLE_NOTEBOOKS]
		elif a.polars:
			targets = [(POLARS_NOTEBOOK, "rnx-polars-check")]
		else:
			targets = [(n, "rnx-check") for n in NOTEBOOKS]
		if a.only:
			targets = [t for t in targets if t[0] in a.only.split(",")]
		for name, kernel_name in targets:
			generate(kernel_name, name)
			print(f"generated {name}.ipynb")
		return
	if a.controls:
		c = Controls()
		c.plain(a.rnx, "rnx-check")
		if a.polars:
			c.polars(a.polars, "rnx-polars-check")
		if a.candle:
			c.candle(a.candle, "rnx-candle-check")
		sys.exit(0 if c.ok else 1)
	failed = False
	where_of = {"rnx-check": "plain rnx", "rnx-polars-check": "the Polars worker", "rnx-candle-check": "the Candle worker"}
	for name, exe, kernel_name, restart in plan:
		where = where_of[kernel_name]
		try:
			n = verify(exe, kernel_name, name, restart=restart)
			print(f"ok: {name} on {where}: {n} runs match the stored outputs and the terminal reference"
				  + (" (one kernel restarted: new pid, earlier state gone)" if restart else "")
				  + ("; every frame and answer matches sales.csv" if name == POLARS_NOTEBOOK else "")
				  + ("; every result matches the recomputed model" if name == "05_candle_model" else "")
				  + ("; every ranking matches the sentence-transformers reference" if name == "06_candle_search" else ""))
		except Mismatch as e:
			failed = True
			print(f"FAIL: {name} on {where}: {e}")
	sys.exit(1 if failed else 0)


if __name__ == "__main__":
	main()
