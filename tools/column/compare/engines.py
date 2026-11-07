"""Prepared eager calls; adaptation and output materialization stay inside call."""
from fixtures import SPECS


def polars_expression(kind, native=False):
	import polars as pl
	x,y,a,b,c,i=(pl.col(n) for n in ('x','y','a','b','c','i'))
	def cmp(v,k,op):
		e=(v>k) if op=='gt' else (v<k)
		return e if native else (~v.is_nan()) & e
	p=cmp(x,1.0,'gt') & cmp(y,2.0,'lt')
	e={'f64const':x+1.0,'f64add':x+y,'f64chain':x*2.0+y,
		'booland':a&b,'boolor':a|b,'boolchain':(a&b)|c,
		'f64gt':cmp(x,1.0,'gt'),'i64eq':i==2,'f64and':p,'mixedor':p|c,
		'arithcmp':cmp(x*2.0+y,3.0,'gt')}[kind]
	if not native and kind in ('booland','boolor','boolchain','f64and','mixedor'):
		e=pl.when(pl.all_horizontal([pl.col(n).is_not_null() for n in SPECS[kind][0]])).then(e).otherwise(None)
	return e.alias('output')


def sql_expression(kind,native=False):
	def cmp(v,k,op):
		e=f'({v}) {op} {k}::DOUBLE'
		return e if native else f'(CASE WHEN isnan({v}) THEN false ELSE {e} END)'
	p=f'({cmp("x",1.0,">")}) AND ({cmp("y",2.0,"<")})'
	e={'f64const':'x+1.0::DOUBLE','f64add':'x+y','f64chain':'x*2.0::DOUBLE+y',
		'booland':'a AND b','boolor':'a OR b','boolchain':'(a AND b) OR c',
		'f64gt':cmp('x',1.0,'>'),'i64eq':'i=2::BIGINT','f64and':p,
		'mixedor':f'({p}) OR c','arithcmp':cmp('x*2.0::DOUBLE+y',3.0,'>')}[kind]
	if not native and kind in ('booland','boolor','boolchain','f64and','mixedor'):
		e='CASE WHEN '+' OR '.join(n+' IS NULL' for n in SPECS[kind][0])+' THEN NULL ELSE ('+e+') END'
	return e


def arrow_call(kind, table):
	import pyarrow as pa
	import pyarrow.compute as pc
	def col(n): return table[n]
	def p(): return pc.and_(pc.greater(col('x'),1.0),pc.less(col('y'),2.0))
	return {'f64const':lambda:pc.add(col('x'),1.0),'f64add':lambda:pc.add(col('x'),col('y')),
		'f64chain':lambda:pc.add(pc.multiply(col('x'),2.0),col('y')),
		'booland':lambda:pc.and_(col('a'),col('b')),'boolor':lambda:pc.or_(col('a'),col('b')),
		'boolchain':lambda:pc.or_(pc.and_(col('a'),col('b')),col('c')),
		'f64gt':lambda:pc.greater(col('x'),1.0),'i64eq':lambda:pc.equal(col('i'),pa.scalar(2,pa.int64())),
		'f64and':p,'mixedor':lambda:pc.or_(p(),col('c')),
		'arithcmp':lambda:pc.greater(pc.add(pc.multiply(col('x'),2.0),col('y')),3.0)}[kind]


def prepare(engine, kind, table, native, threads):
	if engine=='arrow':
		import pyarrow as pa
		assert not native
		if threads=='single': pa.set_cpu_count(1)
		return arrow_call(kind,table), lambda r:r, dict(version=pa.__version__,execution='serial direct kernels',pool_size=pa.cpu_count())
	if engine=='polars':
		import polars as pl
		df=pl.from_arrow(table,rechunk=False)
		expr=polars_expression(kind,native)
		return lambda:df.select(expr), lambda r:r.to_series().to_arrow(), dict(version=pl.__version__,pool_size=pl.thread_pool_size(),input_chunks={n:df[n].n_chunks() for n in df.columns})
	if engine=='duckdb':
		import duckdb
		con=duckdb.connect()
		if threads=='single': con.execute('SET threads=1')
		con.register('incoming',table)
		con.execute('CREATE TABLE inputs AS SELECT * FROM incoming')
		con.unregister('incoming')
		# Prepare SQL outside timing, execute and fully export inside it.
		con.execute('PREPARE measured AS SELECT '+sql_expression(kind,native)+' AS output FROM inputs')
		return lambda:con.execute('EXECUTE measured').to_arrow_table(), lambda r:r.column(0), dict(version=duckdb.__version__,threads=con.execute("SELECT current_setting('threads')").fetchone()[0],input_layout='materialized DuckDB table; offsets/chunks normalized')
	raise ValueError(engine)
