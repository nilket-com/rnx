from pathlib import Path
ROOT=Path(__file__).resolve().parents[2]
KERNELS=('fib','loops','numeric_calls','shared_rows','awfy_sieve','awfy_permute','vec_iter_i64','vec_iter_bool')
def fixture(kind,n):
	if kind in KERNELS:
		return (ROOT/'tools/routing-bench/kernels'/f'{kind}.rn').read_text()+f'\npub fn main(args) {{ let checksum=0; for i in 0..{n} {{checksum=(checksum+kernel(i%97+1))%1000000007;}} checksum}}\n'
	if kind=='host':
		return f'pub fn main(args) {{ let df=polars::DataFrame::empty(); let sum=0;for i in 0..{n} {{sum+=df.width()+i;}} assert_eq!(sum,{n*(n-1)//2});sum}}\n'
	assert kind=='callback'
	return f'''pub fn main(args) {{
	let s=polars::Series::from_iter_option_i64([Some(1)])?.new_from_index(0,{n})?;
	let c=s.i64()?.apply_in_place(|x| x+1)?;
	let total=0; for i in 0..{n} {{assert_eq!(c.get(i)?,Some(2));total+=2;}} assert_eq!(total,{n*2});total
}}
'''
