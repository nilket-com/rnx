#!/usr/bin/env python3
"""Record 0115: every registry list's order, derived independently from the
base source (32d1b40: the order of each site's inline family blocks) and
compared with the lists in families/mod.rs.

  orders.py <repo root> [base commit]
"""
import glob, os, re, subprocess, sys

REC = {'0106': 'layout', '0105': 'owned_iter', '0104': 'view_snapshot', '0103': 'iter_snapshot',
       '0102': 'array_snapshot', '0101': 'indexed_chunk', '0099': 'chunk_snapshot', '0096': 'null_aware',
       '0087': 'iterator_return', '0092': 'scalar_generic', '0097': 'sized_self'}
SLOT = {'cow_ok': 'cow_return', 'bounded_ok': 'bounded_readback', 'bitmap_ok': 'bitmap_return',
        'bitmap_input': 'bitmap_input', 'iter_return': 'iterator_return', 'hash_token': 'hash_token',
        'hash_ret': 'hash_token', 'layout_native': 'layout', 'owned_native': 'owned_iter',
        'view_native': 'view_snapshot', 'iter_native': 'iter_snapshot', 'array_native': 'array_snapshot',
        'indexed_native': 'indexed_chunk', 'chunk_native': 'chunk_snapshot'}


def base(root, commit, path):
    return subprocess.run(['git', '-C', root, 'show', f'{commit}:tools/polars-gen/src/{path}'],
                          capture_output=True, text=True, check=True).stdout


def between(t, a, b):
    i = t.index(a)
    return t[i:t.index(b, i)]


def records(t, indent='\t\t'):
    return [REC[r] for r in re.findall(r'(?m)^' + indent + r'// record (0\d+)', t) if r in REC]


def derive(root, commit):
    m = base(root, commit, 'world/mapping.rs')
    c = base(root, commit, 'emit/callable.rs')
    i = base(root, commit, 'emit/instantiations.rs')
    om = base(root, commit, 'oracle/mod.rs')
    oe = base(root, commit, 'oracle/emit.rs')
    d = {}
    ret = between(m, 'pub(crate) fn ret(&self, t: &Ty', 'if depth > 6 {')
    d['RET_TOP'] = records(ret)
    pre = between(c, 'pub(crate) fn emit_callable(', 'match hash_token_scope')
    d['CALLABLE'] = [SLOT[s] for s in re.findall(r'world\.(\w+)(?:\.set\(|\.borrow_mut\(\) =)', pre)] + ['hash_token']
    loop = between(i, 'for p in pairs {', 'let before = out.entries.len();')
    recs = records(loop, '\t\t')
    d['PAIR_PRE'], d['PAIR_POST'] = recs[:2], recs[2:]
    checks = between(c, "\t// record 0106: the layout's preflight", 'let fallible = fallible || sized_self_op')
    d['CHECK'] = records(checks, '\t')
    ofmt = between(om, 'pub(crate) fn oracle_fmt(', 'match t {')
    d['ORACLE_TOP'] = records(ofmt)
    st = between(oe, 'o.hash_ret.set(', 'let of = match &ret_ty {')
    d['ORACLE_STATE'] = [SLOT[s] for s in re.findall(r'o\.(\w+)(?:\.set\(|\.borrow_mut\(\) =)', st)]
    scal = between(m, '"f32" => r("f64", "(__r as f64)".into(), "float"),', 'p if RISKY_INTS.contains(&p)')
    d['RET_SCALAR'] = [SLOT[s] for s in re.findall(r'self\.(hash_token|bounded_ok)', scal)]
    arg = between(m, '"f32" => ok_arg("f64", format!("({name} as f32)"), "float"),', 'p if INT_NARROW.contains(&p) => ok_arg(')
    d['ARG_SCALAR'] = ['arg_guard' if s == 'arg_guard' else SLOT[s] for s in re.findall(r'\.(arg_guard|hash_token)\n', arg)]
    pl = base(root, commit, 'pipeline.rs')
    loop = between(pl, 'for c in callables {', 'let op_used = emit_op_groups(')
    marks = [('emit_instantiations(', 'instantiation_route'), ('// record 0077', 'iterator_return_route'),
             ('if !api {', 'out_of_scope'), ('if index_row(c)', 'index'), ('if from_iter_row(c)', 'from_iter'),
             ('if unary_generic_shape(c)', 'unary'), ('if generic_op(c)', 'operator'),
             ('generic_impl_refusal(c)', 'generic_impl_refusal')]
    d['CLAIM'] = [n for _, n in sorted((loop.index(m), n) for m, n in marks)]
    return d


def registry(root):
    src = os.path.join(root, 'tools/polars-gen/src')
    names = {}
    for f in glob.glob(os.path.join(src, 'families', '*.rs')):
        t = open(f).read()
        structs = dict(re.findall(r'pub\(crate\) static (\w+): (\w+) = \w+;', t))
        for s, n in re.findall(r'impl Family for (\w+) \{\n\tfn name\(&self\) -> &\'static str \{\n\t\t"(\w+)"', t):
            for stat, st in structs.items():
                if st == s:
                    names[stat] = n
    mod = open(os.path.join(src, 'families', 'mod.rs')).read()
    lists = {}
    for name, body in re.findall(r'pub\(crate\) static (\w+): &\[&dyn Family\] =\s*&\[(.*?)\];', mod, re.S):
        lists[name] = [names[x] for x in re.findall(r'&(\w+)', body)]
    return lists


if __name__ == '__main__':
    root = sys.argv[1]
    commit = sys.argv[2] if len(sys.argv) > 2 else '32d1b40'
    d, r = derive(root, commit), registry(root)
    bad = 0
    for k, v in d.items():
        ok = r.get(k) == v
        bad += not ok
        print(f"{'ok ' if ok else 'DIFFERS'} {k}: base {v}" + ('' if ok else f'\n        registry {r.get(k)}'))
    print('orders: ' + ('every derived list equals the registry' if not bad else f'{bad} differ'))
    sys.exit(1 if bad else 0)
