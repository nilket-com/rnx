"""Ordinary script fixtures; all construction and equality checks are outside map."""
import json

KERNELS = {
	"i64add": ("i64", "pub fn map(x) { x + 1 }", ["1"], "2"),
	"axpy": ("i64", "pub fn map(x,y) { x * 2 + y }", ["1", "2"], "4"),
	"multiply3": ("i64", "pub fn map(x,y) { x * 3 + y }", ["1", "2"], "5"),
	"f64const": ("f64", "pub fn map(x) { x + 1.0 }", ["1.0"], "2.0"),
	"f64add": ("f64", "pub fn map(x,y) { x + y }", ["1.0", "2.0"], "3.0"),
	"boolnot": ("bool", "pub fn map(x) { !x }", ["true"], "false"),
}


def legacy_fixture(kind, rows, layout, calls=2):
	ty, source, inputs, expected = KERNELS[kind]
	assert rows >= 8 and layout in ("flat", "sliced-chunked-null")
	dtype = {"i64": "Int64", "f64": "Float64", "bool": "Boolean"}[ty]
	lines = ["pub fn main(args) {"]
	def column(name, value, nullable=False, offset=0):
		seed = f'polars::Series::from_iter_option_{ty}([Some({value})])?'
		if nullable:
			lines.append(f'let {name} = {seed}.new_from_index(0, {rows // 2 + offset})?;')
			lines.append(f'{name}.append(polars::Series::full_null("", {rows - rows // 2}, polars::DataType::{dtype}())?)?;')
		else:
			lines.append(f'let {name} = {seed}.new_from_index(0, {rows + offset})?;')
		if offset:
			lines.append(f'let {name} = {name}.slice({offset}, {rows})?;')
	for i, value in enumerate(inputs):
		column("input" + str(i), value, layout != "flat" and i == 0, 3 + i * 2 if layout != "flat" else 0)
	column("expected", expected, layout != "flat", 3 if layout != "flat" else 0)
	lines += [
		"let prep_start = time::monotonic_ms();",
		f'let k = polars::column_kernel({json.dumps(source)}, "map", {json.dumps([ty] * len(inputs))}, "{ty}")?;',
		"let prep_ms = time::monotonic_ms() - prep_start;",
		'println!("COLUMN_PREP ms={} {}", prep_ms, k.explain());',
		f'for call in 0..{calls} {{',
		'println!("COLUMN_ENTER {}", call);',
		"let started = time::monotonic_ms();",
		'let output = k.map([' + ', '.join('input' + str(i) for i in range(len(inputs))) + '])?;',
		"let elapsed_ms = time::monotonic_ms() - started;",
		"assert!(output.equals_missing(expected));",
		f"assert_eq!(output.len()?, {rows});",
		'println!("COLUMN_OK call={} ms={} {}", call, elapsed_ms, k.last_path());',
		"}", "Ok(())", "}",
	]
	return "\n".join(lines) + "\n"


LAYOUTS = ("flat", "sliced-chunked-null", "alternating", "deterministic-random")


def present(pattern, index, argument):
	if pattern == "alternating":
		return index % 2 == 0
	assert pattern == "deterministic-random"
	return ((index % 256) * 73 + argument * 41 + 19) % 251 < 127


def valid_count(kind, rows, layout):
	if layout == "flat":
		return rows
	if layout == "sliced-chunked-null":
		return rows // 2
	arity = len(KERNELS[kind][2])
	period = [all(present(layout, i, a) for a in range(arity)) for i in range(256)]
	return rows // 256 * sum(period) + sum(period[:rows % 256])


def fixture(kind, rows, layout, calls=2):
	if layout in ("flat", "sliced-chunked-null"):
		return legacy_fixture(kind, rows, layout, calls)
	assert rows >= 8 and layout in LAYOUTS
	ty, source, inputs, expected = KERNELS[kind]
	lines = ["pub fn main(args) {"]
	def column(name, value, argument, offset):
		def valid(i):
			if argument is None:
				return all(present(layout, i, a) for a in range(len(inputs)))
			return present(layout, i, argument)
		seed = ", ".join(f"Some({value})" if valid((i - offset) % 256) else "None" for i in range(256))
		lines.append(f"let {name} = polars::Series::from_iter_option_{ty}([{seed}])?;")
		lines.append(f"while {name}.len()? < {rows + offset} {{")
		lines.append(f"let extra = {name}.clone(); {name}.append(extra)?;")
		lines.append(f"let compact = {name}.rechunk(); {name} = compact;")
		lines.append("}")
		lines.append(f"let {name}_full = {name}.slice({offset}, {rows})?;")
		# Independent physical chunks with non-byte-aligned boundaries/tails.
		cut = rows // 3 + offset
		end = rows - 7 - offset
		lines.append(f"let {name} = {name}_full.slice(0, {cut})?;")
		lines.append(f"{name}.append({name}_full.slice({cut}, {end - cut})?)?;")
		lines.append(f"{name}.append({name}_full.slice({end}, {rows - end})?)?;")
	for i, value in enumerate(inputs):
		column("input" + str(i), value, i, 3 + i * 2)
	column("expected", expected, None, 1)
	lines += [
		f'let k = polars::column_kernel({json.dumps(source)}, "map", {json.dumps([ty] * len(inputs))}, "{ty}")?;',
		f"for call in 0..{calls} {{",
		'println!("COLUMN_ENTER {}", call);',
		"let output = k.map([" + ", ".join("input" + str(i) for i in range(len(inputs))) + "])?;",
		"assert!(output.equals_missing(expected));",
		f"assert_eq!(output.len()?, {rows});",
		'println!("COLUMN_OK call={} {}", call, k.last_path());',
		"}", "Ok(())", "}",
	]
	return "\n".join(lines) + "\n"
