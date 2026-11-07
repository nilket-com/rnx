"""ADR 0223 numeric predicates with typed inputs and packed expected outputs."""
import json
SPECS={
	'f64gt':('f64','pub fn map(x) { x > 1.0 }',['2.0'],'true',['f64']),
	'f64le':('f64','pub fn map(x) { x <= 1.0 }',['2.0'],'false',['f64']),
	'i64eq':('i64','pub fn map(x) { x == 2 }',['2'],'true',['i64']),
	'f64and':('f64','pub fn map(x,y) { (x > 1.0) & (y < 2.0) }',['2.0','1.0'],'true',['f64','f64']),
	'mixedor':('f64','pub fn map(x,y,z) { ((x > 1.0) & (y < 2.0)) | z }',['2.0','1.0','true'],'true',['f64','f64','bool']),
	'arithcmp':('f64','pub fn map(x,y) { x * 2.0 + y > 3.0 }',['2.0','1.0'],'true',['f64','f64']),
}
def install(c,f):
	original=f.fixture
	for k,s in SPECS.items():f.KERNELS[k]=s[:4]
	def fixture(kind,rows,layout,calls=2):
		text=original(kind,rows,layout,calls)
		if kind not in SPECS:return text
		ty,source,inputs,expected,types=SPECS[kind]
		lines=[]
		for line in text.splitlines():
			if line.startswith('let expected = polars::Series::from_iter_option_'):
				line=line.replace('from_iter_option_'+ty,'from_iter_option_bool')
			if line.startswith('expected.append('):
				line=line.replace('DataType::Float64','DataType::Boolean').replace('DataType::Int64','DataType::Boolean')
			if kind=='mixedor' and line.startswith('let input2 = polars::Series::from_iter_option_'):
				line=line.replace('from_iter_option_f64','from_iter_option_bool')
			if 'let k = polars::column_kernel(' in line:
				line=f'let k = polars::column_kernel({json.dumps(source)}, "map", {json.dumps(types)}, "bool")?;'
			lines.append(line)
		return '\n'.join(lines)+'\n'
	f.fixture=fixture;c.fixture=fixture
	return fixture
