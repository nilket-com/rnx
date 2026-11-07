"""Small installed-engine semantic controls; no performance measurements."""
import argparse
import hashlib
import json
import math
from pathlib import Path
import struct
import sys
import traceback

KINDS = ('f64const', 'f64add', 'f64chain', 'booland', 'boolor', 'boolchain',
	'f64gt', 'i64eq', 'f64and', 'mixedor', 'arithcmp')
INPUTS = {'f64const': ('x',), 'f64add': ('x', 'y'), 'f64chain': ('x', 'y'),
	'booland': ('a', 'b'), 'boolor': ('a', 'b'), 'boolchain': ('a', 'b', 'c'),
	'f64gt': ('x',), 'i64eq': ('i',), 'f64and': ('x', 'y'),
	'mixedor': ('x', 'y', 'c'), 'arithcmp': ('x', 'y')}


def oracle(kind, row):
	if any(row[n] is None for n in INPUTS[kind]):
		return None
	x, y, a, b, c, i = (row[n] for n in ('x', 'y', 'a', 'b', 'c', 'i'))
	return {'f64const': lambda: x + 1.0, 'f64add': lambda: x + y,
		'f64chain': lambda: x * 2.0 + y, 'booland': lambda: a & b,
		'boolor': lambda: a | b, 'boolchain': lambda: (a & b) | c,
		'f64gt': lambda: x > 1.0, 'i64eq': lambda: i == 2,
		'f64and': lambda: (x > 1.0) & (y < 2.0),
		'mixedor': lambda: ((x > 1.0) & (y < 2.0)) | c,
		'arithcmp': lambda: x * 2.0 + y > 3.0}[kind]()


def equal(a, b):
	if a is None or b is None:
		return a is b
	if isinstance(a, float) and isinstance(b, float):
		return (math.isnan(a) and math.isnan(b)) or struct.pack('>d', a) == struct.pack('>d', b)
	return type(a) is type(b) and a == b


def probe():
	import duckdb
	import polars as pl
	import pyarrow as pa
	import pyarrow.compute as pc
	import itertools
	rows = []
	for a, b, c in itertools.product((None, False, True), repeat=3):
		rows.append(dict(x=2.0, y=1.0, a=a, b=b, c=c, i=2))
	for x, y in itertools.product((None, float('nan'), float('inf'), -float('inf'),
		-0.0, 0.0, 1.0, 2.0, 1e308, 0.1), repeat=2):
		rows.append(dict(x=x, y=y, a=True, b=False, c=True, i=-2**63))
	# Cancellation-sensitive multiply/add and signed-zero controls.
	rows += [dict(x=-0.0, y=-0.0, a=False, b=True, c=False, i=2**63-1),
		dict(x=1.0000000000000002, y=-2.0, a=True, b=True, c=False, i=None)]
	table = pa.table({n: pa.array([r[n] for r in rows], type=pa.float64() if n in ('x', 'y')
		else pa.int64() if n == 'i' else pa.bool_()) for n in rows[0]})
	df = pl.from_arrow(table)
	con = duckdb.connect()
	con.register('inputs', table)
	def pexpr(kind, native):
		x, y, a, b, c, i = (pl.col(n) for n in ('x', 'y', 'a', 'b', 'c', 'i'))
		def gt(v, k):
			return (v > k) if native else (~v.is_nan()) & (v > k)
		def lt(v, k):
			return (v < k) if native else (~v.is_nan()) & (v < k)
		p = gt(x, 1.0) & lt(y, 2.0)
		e = {'f64const': x+1.0, 'f64add': x+y, 'f64chain': x*2.0+y,
			'booland': a&b, 'boolor': a|b, 'boolchain': (a&b)|c,
			'f64gt': gt(x,1.0), 'i64eq': i==2, 'f64and': p,
			'mixedor': p|c, 'arithcmp': gt(x*2.0+y,3.0)}[kind]
		if not native:
			e = pl.when(pl.all_horizontal([pl.col(n).is_not_null() for n in INPUTS[kind]])).then(e).otherwise(None)
		return df.select(e.alias('out')).to_series().to_list()
	def sql(kind, native):
		def gt(v, k):
			e = f'({v}) > {k}::DOUBLE'
			return e if native else f'(CASE WHEN isnan({v}) THEN false ELSE {e} END)'
		def lt(v, k):
			e = f'({v}) < {k}::DOUBLE'
			return e if native else f'(CASE WHEN isnan({v}) THEN false ELSE {e} END)'
		p = f'({gt("x",1.0)}) AND ({lt("y",2.0)})'
		e = {'f64const':'x+1.0::DOUBLE', 'f64add':'x+y', 'f64chain':'x*2.0::DOUBLE+y',
			'booland':'a AND b', 'boolor':'a OR b', 'boolchain':'(a AND b) OR c',
			'f64gt':gt('x',1.0), 'i64eq':'i=2::BIGINT', 'f64and':p,
			'mixedor':f'({p}) OR c', 'arithcmp':gt('x*2.0::DOUBLE+y',3.0)}[kind]
		if not native:
			e = 'CASE WHEN ' + ' OR '.join(n+' IS NULL' for n in INPUTS[kind]) + ' THEN NULL ELSE (' + e + ') END'
		return [r[0] for r in con.execute('SELECT '+e+' FROM inputs').fetchall()]
	def arrow(kind):
		x,y,a,b,c,i = (table[n] for n in ('x','y','a','b','c','i'))
		p = lambda: pc.and_(pc.greater(x,1.0),pc.less(y,2.0))
		return {'f64const':lambda:pc.add(x,1.0), 'f64add':lambda:pc.add(x,y),
			'f64chain':lambda:pc.add(pc.multiply(x,2.0),y), 'booland':lambda:pc.and_(a,b),
			'boolor':lambda:pc.or_(a,b), 'boolchain':lambda:pc.or_(pc.and_(a,b),c),
			'f64gt':lambda:pc.greater(x,1.0), 'i64eq':lambda:pc.equal(i,pa.scalar(2,pa.int64())),
			'f64and':p, 'mixedor':lambda:pc.or_(p(),c),
			'arithcmp':lambda:pc.greater(pc.add(pc.multiply(x,2.0),y),3.0)}[kind]().to_pylist()
	checks=[]
	for kind in KINDS:
		expected=[oracle(kind,r) for r in rows]
		for engine, function in [('polars',lambda:pexpr(kind,False)),('duckdb',lambda:sql(kind,False)),('arrow',lambda:arrow(kind))]:
			actual=function()
			bad=[j for j,(a,b) in enumerate(zip(actual,expected)) if not equal(a,b)]
			assert len(actual)==len(expected) and not bad, (engine,kind,bad[:8])
			checks.append(dict(engine=engine,kind=kind,rows=len(rows),status='PASS'))
	differences={engine:{kind:sum(not equal(a,oracle(kind,r)) for a,r in zip(fn(kind,True),rows))
		for kind in KINDS} for engine,fn in [('polars',pexpr),('duckdb',sql)]}
	return dict(status='PASS',versions=dict(polars=pl.__version__,pyarrow=pa.__version__,duckdb=duckdb.__version__),
		checks=checks,native_mismatch_counts=differences,
		observations=dict(arrow_nan_gt=pc.greater(pa.array([float('nan')]),1.0).to_pylist(),
			polars_nan_gt=pl.Series([float('nan')]).gt(1.0).to_list(),
			duckdb_nan_gt=con.execute("SELECT 'NaN'::DOUBLE > 1.0").fetchone()[0]),
		limitations='Small semantic controls only; not a timing, full layout, or rnx cross-runtime run.')


def main():
	parser=argparse.ArgumentParser(description=__doc__)
	parser.add_argument('--out',type=Path,required=True)
	args=parser.parse_args()
	try:
		result=probe()
	except Exception:
		result=dict(status='FAIL',error=traceback.format_exc())
	result['source_sha256']=hashlib.sha256(Path(__file__).read_bytes()).hexdigest()
	args.out.parent.mkdir(parents=True,exist_ok=True)
	args.out.write_text(json.dumps(result,indent=2)+'\n')
	print(result['status']+': '+str(args.out))
	return 0 if result['status']=='PASS' else 1


if __name__=='__main__':
	sys.exit(main())
