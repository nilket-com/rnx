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
import argparse, copy, csv, html.parser, json, os, pathlib, re, shutil, subprocess, sys, tempfile

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
	reference = terminal(rnx, name)
	if name == POLARS_NOTEBOOK:
		check_sales_answer(reference, sales_expected(SALES))
		check_sales(stored, SALES)
	runs = execute(stored, kernel_name, restart=restart) + execute(stored, kernel_name)
	for run in runs:
		stdout = judge(run)
		if stdout != reference:
			raise Mismatch(f"{name}: the notebook's stdout differs from the terminal demo:\n{stdout!r}\n{reference!r}")
		compare(stored, run)
		if name == POLARS_NOTEBOOK:
			check_sales(run, SALES)
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


def parse_frame(text, whole):
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
	cells = [[parse_cell(c, d) for c, (_, d) in zip(r, schema)] for r in data]
	return height, width, schema, cells


def parse_cell(text, dtype):
	if dtype == "string":
		# unquoted (record 0158); this example's strings are simple, so `…` is refused
		if not re.fullmatch(r"[A-Za-z0-9_-]+", text):
			raise Mismatch(f"not a simple string cell: {text!r}")
		return text
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


def check_frame(text, label, height, schema, rows):
	"""Whole frames exactly; a taller one by its first and last five rows (record 0158)."""
	whole = height <= ROWS_SHOWN
	h, w, shown_schema, cells = parse_frame(text, whole)
	if (h, w) != (height, len(schema)):
		raise Mismatch(f"{label}: {h} × {w}, expected {height} × {len(schema)}")
	if shown_schema != schema:
		raise Mismatch(f"{label}: schema {shown_schema}, expected {schema}")
	expected = rows if whole else rows[:HALF] + rows[-HALF:]
	if cells != expected:
		raise Mismatch(f"{label}: rows differ:\n  shown    {cells}\n  expected {expected}")


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


def parse_html_frame(form, whole):
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
	cells = [[parse_cell(c, d) for c, (_, d) in zip(r, schema)] for r in data]
	return height, width, schema, cells


def check_frame_html(form, label, height, schema, rows):
	whole = height <= ROWS_SHOWN
	h, w, shown_schema, cells = parse_html_frame(form, whole)
	if (h, w) != (height, len(schema)) or shown_schema != schema:
		raise Mismatch(f"{label} (HTML): {h} × {w} {shown_schema}, expected {height} × {len(schema)} {schema}")
	expected = rows if whole else rows[:HALF] + rows[-HALF:]
	if cells != expected:
		raise Mismatch(f"{label} (HTML): rows differ:\n  shown    {cells}\n  expected {expected}")


def check_sales(nb, data):
	"""Every displayed frame, as text and as HTML, then the printed answer, against the CSV."""
	expected = sales_expected(data)
	forms = html_results(nb)  # first: it refuses an HTML form without its text fallback
	texts = results(nb)
	if len(texts) != len(expected["frames"]):
		raise Mismatch(f"{len(texts)} results shown, expected {len(expected['frames'])} frames")
	for text, form, (label, height, schema, rows) in zip(texts, forms, expected["frames"]):
		check_frame(text, label, height, schema, rows)
		check_frame_html(form, label, height, schema, rows)
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


def generate(kernel_name, name):
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


class Controls:
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
	mode = p.add_mutually_exclusive_group()
	mode.add_argument("--generate", action="store_true")
	mode.add_argument("--controls", action="store_true")
	a = p.parse_args()
	for exe in (a.rnx, a.kernel) + ((a.polars,) if a.polars else ()):
		if not os.path.isabs(exe) or not os.access(exe, os.X_OK):
			sys.exit(f"{exe}: give an absolute path to an executable")
	workers = {"rnx-check": a.rnx}
	if a.polars:
		workers["rnx-polars-check"] = a.polars
	kernelspecs(workers, a.kernel)
	# (notebook, terminal executable, kernel, restart): the plain set on plain rnx, then, with
	# --polars, the Polars notebook and the plain set again on the Polars worker
	plan = [(name, a.rnx, "rnx-check", name == "02_orders") for name in NOTEBOOKS]
	if a.polars:
		plan += [(POLARS_NOTEBOOK, a.polars, "rnx-polars-check", True)]
		plan += [(name, a.polars, "rnx-polars-check", False) for name in NOTEBOOKS]
	if a.generate:
		# with --polars only the Polars notebook is written; the plain ones come from plain rnx
		targets = [(POLARS_NOTEBOOK, "rnx-polars-check")] if a.polars else [(n, "rnx-check") for n in NOTEBOOKS]
		for name, kernel_name in targets:
			generate(kernel_name, name)
			print(f"generated {name}.ipynb")
		return
	if a.controls:
		c = Controls()
		c.plain(a.rnx, "rnx-check")
		if a.polars:
			c.polars(a.polars, "rnx-polars-check")
		sys.exit(0 if c.ok else 1)
	failed = False
	for name, exe, kernel_name, restart in plan:
		where = "plain rnx" if kernel_name == "rnx-check" else "the Polars worker"
		try:
			n = verify(exe, kernel_name, name, restart=restart)
			print(f"ok: {name} on {where}: {n} runs match the stored outputs and the terminal reference"
				  + (" (one kernel restarted: new pid, earlier state gone)" if restart else "")
				  + ("; every frame and answer matches sales.csv" if name == POLARS_NOTEBOOK else ""))
		except Mismatch as e:
			failed = True
			print(f"FAIL: {name} on {where}: {e}")
	sys.exit(1 if failed else 0)


if __name__ == "__main__":
	main()
