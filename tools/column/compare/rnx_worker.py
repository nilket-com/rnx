"""Generate production-CLI scripts with construction/validation outside map time."""
import json
from fixtures import expected_period


def literal(v):
	if v is None: return 'None'
	if isinstance(v,bool): return 'Some('+str(v).lower()+')'
	return 'Some('+repr(v)+')'


def script(m, calls, timed):
	lines=['pub fn main(args) {']
	rows=m['rows']
	def column(name,ty,values,offset,cuts):
		lines.append(f'let {name} = polars::Series::from_iter_option_{ty}(['+', '.join(literal(v) for v in values)+'])?;')
		if len(values)==1:
			lines.append(f'let {name} = {name}.new_from_index(0, {rows+offset})?;')
		else:
			lines.extend([f'while {name}.len()? < {rows+offset} {{',f'let extra = {name}.clone(); {name}.append(extra)?;',f'let compact = {name}.rechunk(); {name} = compact;','}'])
		lines.append(f'let {name}_full = {name}.slice({offset}, {rows})?;')
		lines.append(f'let {name} = {name}_full.slice(0, {cuts[1]})?;')
		for a,b in zip(cuts[1:],cuts[2:]): lines.append(f'{name}.append({name}_full.slice({a}, {b-a})?)?;')
	for index,col in enumerate(m['columns']):
		name='input'+str(index); off=col['offset']; period=col['validity_period']
		values=[col['value'] if period[(i-off)%len(period)] else None for i in range(256)] if len(period)>1 else [col['value']]
		if col['null_after'] is not None:
			dtype={'f64':'Float64','i64':'Int64','bool':'Boolean'}[col['type']]
			lines.append(f'let {name} = polars::Series::from_iter_option_{col["type"]}([{literal(col["value"])}])?.new_from_index(0, {rows//2+off})?;')
			lines.append(f'{name}.append(polars::Series::full_null("", {rows-rows//2}, polars::DataType::{dtype}())?)?;')
			lines.append(f'let {name} = {name}.slice({off}, {rows})?;')
		else: column(name,col['type'],values,off,col['cuts'])
	if m['layout']=='sliced-chunked-null':
		v=expected_period(m,0); dtype='Float64' if m['output']=='f64' else 'Boolean'
		lines.append(f'let expected = polars::Series::from_iter_option_{m["output"]}([{literal(v)}])?.new_from_index(0, {rows//2})?;')
		lines.append(f'expected.append(polars::Series::full_null("", {rows-rows//2}, polars::DataType::{dtype}())?)?;')
	else:
		values=[expected_period(m,i) for i in range(256)] if m['layout']!='flat' else [expected_period(m,0)]
		column('expected',m['output'],values,0,[0,rows])
	names=[c['name'] for c in m['columns']]
	source='pub fn map('+','.join(names)+') { '+m['expression']+' }'
	lines.append(f'let k = polars::column_kernel({json.dumps(source)}, "map", {json.dumps([c["type"] for c in m["columns"]])}, "{m["output"]}")?;')
	lines.append(f'for call in 0..{calls} {{')
	if timed: lines.append('let started = time::monotonic_ns();')
	lines.append('let output = k.map(['+', '.join('input'+str(i) for i in range(len(names)))+'])?;')
	if timed: lines.append('let elapsed = time::monotonic_ns() - started;')
	lines.extend(['assert!(output.equals_missing(expected));',f'assert_eq!(output.len()?, {rows});'])
	lines.append('println!("SAMPLE {} {}", call, elapsed);' if timed else 'println!("VALIDATED {}", call);')
	lines.extend(['}','Ok(())','}'])
	return '\n'.join(lines)+'\n'
