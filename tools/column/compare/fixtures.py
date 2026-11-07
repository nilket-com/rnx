"""Typed, periodic fixtures shared by all four column engines."""
import hashlib
import importlib.util
import json
from pathlib import Path
import struct

ROOT = Path(__file__).resolve().parents[3]
SOURCE = ROOT / 'rune/tools/xtask/column/cli_fixtures.py'
spec = importlib.util.spec_from_file_location('column_fixture_source', SOURCE)
legacy = importlib.util.module_from_spec(spec)
spec.loader.exec_module(legacy)
LAYOUTS = legacy.LAYOUTS
# Names, types, values and expressions reproduce the previous measured fixtures.
SPECS = {
	'f64const': (['x'], ['f64'], [1.0], 'x + 1.0', 'f64'),
	'f64add': (['x','y'], ['f64']*2, [1.0,2.0], 'x + y', 'f64'),
	'f64chain': (['x','y'], ['f64']*2, [1.0,2.0], 'x * 2.0 + y', 'f64'),
	'booland': (['a','b'], ['bool']*2, [True,False], 'a & b', 'bool'),
	'boolor': (['a','b'], ['bool']*2, [True,False], 'a | b', 'bool'),
	'boolchain': (['a','b','c'], ['bool']*3, [True,False,True], '(a & b) | c', 'bool'),
	'f64gt': (['x'], ['f64'], [2.0], 'x > 1.0', 'bool'),
	'i64eq': (['i'], ['i64'], [2], 'i == 2', 'bool'),
	'f64and': (['x','y'], ['f64']*2, [2.0,1.0], '(x > 1.0) & (y < 2.0)', 'bool'),
	'mixedor': (['x','y','c'], ['f64','f64','bool'], [2.0,1.0,True], '((x > 1.0) & (y < 2.0)) | c', 'bool'),
	'arithcmp': (['x','y'], ['f64']*2, [2.0,1.0], 'x * 2.0 + y > 3.0', 'bool'),
}
NATIVE = ('booland','boolor','boolchain','f64gt','f64and','mixedor','arithcmp')


def manifest(kind, rows, layout):
	assert kind in SPECS and layout in LAYOUTS and rows >= 64
	names, types, values, expr, output = SPECS[kind]
	columns=[]
	for a, (name, ty, value) in enumerate(zip(names, types, values)):
		offset = 0 if layout == 'flat' else 3 + 2*a
		if layout in ('alternating', 'deterministic-random'):
			cuts=[0, rows//3+offset, rows-7-offset, rows]
			period=[legacy.present(layout,i,a) for i in range(256)]
		else:
			cuts=[0, rows//2, rows] if layout=='sliced-chunked-null' and a==0 else [0, rows]
			period=[True]
		columns.append(dict(name=name,type=ty,value=value,offset=offset,cuts=cuts,validity_period=period,
			null_after=rows//2 if layout=='sliced-chunked-null' and a==0 else None))
	m=dict(schema=1,kind=kind,rows=rows,layout=layout,columns=columns,expression=expr,output=output,
		fixture_source_sha256=hashlib.sha256(SOURCE.read_bytes()).hexdigest())
	m['sha256']=hashlib.sha256(json.dumps(m,sort_keys=True).encode()).hexdigest()
	return m


def bit_buffer(values, count):
	assert len(values) in (1,256)
	period = values*256 if len(values)==1 else values
	block=bytes(sum(int(period[i+j]) << j for j in range(8)) for i in range(0,256,8))
	return (block*((count+255)//256))[:(count+7)//8]


def arrow_inputs(m):
	import pyarrow as pa
	out={}
	for col in m['columns']:
		offset=col['offset']; size=m['rows']+offset; ty=col['type']
		period=col['validity_period']; rotated=[period[(i-offset)%len(period)] for i in range(256)]
		valid=None if all(rotated) else pa.py_buffer(bit_buffer(rotated,size))
		data=bit_buffer([col['value']],size) if ty=='bool' else struct.pack('<d' if ty=='f64' else '<q',col['value'])*size
		dtype={'bool':pa.bool_(),'f64':pa.float64(),'i64':pa.int64()}[ty]
		base=pa.Array.from_buffers(dtype,size,[valid,pa.py_buffer(data)]).slice(offset,m['rows'])
		chunks=[]
		for start,end in zip(col['cuts'],col['cuts'][1:]):
			chunks.append(pa.nulls(end-start,type=dtype) if col['null_after'] is not None and start>=col['null_after'] else base.slice(start,end-start))
		out[col['name']]=pa.chunked_array(chunks)
	return pa.table(out)


def scalar(kind, values, native=False):
	from semantics import oracle
	row=dict(x=0.0,y=0.0,a=False,b=False,c=False,i=0)
	row.update(values)
	if not native:
		return oracle(kind,row)
	def and_(a,b):
		return False if a is False or b is False else None if a is None or b is None else True
	def or_(a,b):
		return True if a is True or b is True else None if a is None or b is None else False
	if kind=='booland': return and_(row['a'],row['b'])
	if kind=='boolor': return or_(row['a'],row['b'])
	if kind=='boolchain': return or_(and_(row['a'],row['b']),row['c'])
	p=None if row['x'] is None else row['x']>1.0
	q=None if row['y'] is None else row['y']<2.0
	if kind=='f64and': return and_(p,q)
	if kind=='mixedor': return or_(and_(p,q),row['c'])
	return oracle(kind,row)  # Measured fixtures contain only finite constants.


def expected_period(m, position, native=False):
	values={}
	for col in m['columns']:
		valid=col['validity_period'][position%len(col['validity_period'])]
		if col['null_after'] is not None and position>=col['null_after']: valid=False
		values[col['name']]=col['value'] if valid else None
	return scalar(m['kind'],values,native)


def validate(m, result, native=False):
	"""Compare every output lane; bounded temporary storage, independent scalar oracle."""
	import pyarrow as pa
	dtype=pa.float64() if m['output']=='f64' else pa.bool_()
	assert len(result)==m['rows'] and result.type==dtype
	if isinstance(result,pa.Array): result=pa.chunked_array([result])
	digest=hashlib.sha256(); nulls=0
	# Periods are computed once per null regime; never an O(N) Python row loop.
	for begin,end in zip([0,m['rows']//2],[m['rows']//2,m['rows']]):
		period=[expected_period(m,begin+i,native) for i in range(256)]
		for start in range(begin,end,65536):
			count=min(65536,end-start)
			expected=pa.array((period*((count+255)//256))[:count],type=dtype)
			actual=result.slice(start,count).combine_chunks()
			assert actual.equals(expected), (m['kind'],m['layout'],start)
			nulls+=actual.null_count
			# Hash the validated logical oracle encoding, independent of null payload.
			digest.update(str(start).encode());digest.update(str(count).encode())
			digest.update(json.dumps(period).encode())
	return dict(rows=m['rows'],null_count=nulls,logical_sha256=digest.hexdigest())
