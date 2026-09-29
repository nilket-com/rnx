#!/usr/bin/env python3
"""Record 0115: synthetic overlap cases. Each case is a release file (the
shipped 0.55.2-joins.toml with a few listings added or changed, same
provenance) and an inventory (the pinned 0.55.2 adapter-narrow inventory,
plus synthetic callables where no real callable has the needed shape). Each
forces one combination of listed families that the shipped releases never
produce, or one family outcome the repository's generating runs never reach.
`expect` names groups of trace lines (hook, family, outcome) that must all
occur on one concrete target (the callable key, or `key|alias` for a pair)
matching the group's pattern, so two states are proven to meet on the same
callable or pair; a group named `!…` must not occur at all on its pattern.
A case that stops short of its combination fails instead of passing.

  overlap.py <out dir>                 writes <case>/release.toml, inventory.json, expect.tsv
  overlap.py --check <case dir> <trace> checks the trace against the case's expect.tsv
  overlap.py --self-test
"""
import copy, json, os, re, sys

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
RELEASE = os.path.join(ROOT, 'tools/polars-gen/releases/0.55.2-joins.toml')
INVENTORY = os.path.join(ROOT, 'probes/0072/out/0.55.2-adapter-narrow/result/inventory.json')
CA = 'polars_core::chunked_array::ChunkedArray'


def blocks(text):
    """The release text split at top-level `[[table]]`/`[table]` headers."""
    parts = re.split(r'(?m)^(?=\[)', text)
    return parts


def edit_block(text, table, key, fn):
    """Apply `fn` to the one `[[table]]` block whose text names `key`."""
    parts = blocks(text)
    hits = [i for i, p in enumerate(parts) if p.startswith(f'[[{table}]]\n') and f'"{key}"' in p]
    assert len(hits) == 1, (table, key, len(hits))
    parts[hits[0]] = fn(parts[hits[0]])
    return ''.join(parts)


def drop_block(text, table, key):
    return edit_block(text, table, key, lambda b: '')


def pairs_of(text, table, key):
    b = [p for p in blocks(text) if p.startswith(f'[[{table}]]\n') and f'"{key}"' in p]
    assert len(b) == 1, (table, key)
    m = re.search(r'(?ms)^pairs = (\[.*?\]\])\s*$', b[0])
    assert m, b[0][:200]
    return m.group(1)


def add(table, key, path, pairs, extra=''):
    return f'\n[[{table}]]\nkey = "{key}"\npath = "{path}"\n{extra}pairs = {pairs}\ncite = "record 0115 overlap control"\n'


def synthetic(inv, src_key, new_key, name, ret):
    """A clone of `src_key` with its own key, name and return: the only
    change is the shape the case needs."""
    c = copy.deepcopy(next(x for x in inv['callables'] if x['key'] == src_key))
    c['key'] = new_key
    c['name'] = name
    c['canonical_path'] = f'{CA}::{name}'
    c['ret_canonical'] = ret
    c['ret'] = ret
    return c


def G(group, target, *lines):
    return [(group, h, f, o, target) for h, f, o in lines]


def cases(text, inv):
    """(name, release text, inventory, expectations, why)."""
    out = []
    # O1: an arg family and a ret family both answer on one callable
    inv1 = copy.deepcopy(inv)
    inv1['callables'] += [
        synthetic(inv, 'polars_core:3696', 'synthetic:0115-1', 'with_validity_cow', 'alloc::borrow::Cow<Self>'),
        synthetic(inv, 'polars_core:3696', 'synthetic:0115-2', 'with_validity_len', 'usize'),
    ]
    t = text + (f'\n[[bitmap_inputs]]\npath = "{CA}::with_validity_cow"\nlength = "receiver"\ncite = "record 0115 overlap control"\n'
                f'\n[[cow_returns]]\nkey = "synthetic:0115-1"\npath = "{CA}::with_validity_cow"\ncite = "record 0115 overlap control"\n'
                f'\n[[bitmap_inputs]]\npath = "{CA}::with_validity_len"\nlength = "receiver"\ncite = "record 0115 overlap control"\n'
                f'\n[[bounded_readbacks]]\nkey = "synthetic:0115-2"\npath = "{CA}::with_validity_len"\ncite = "record 0115 overlap control"\n')
    out.append(('o1_arg_and_ret', t, inv1,
                G('cow', 'synthetic:0115-1|*', ('scope', 'cow_return+bitmap_input', 'entered'),
                  ('arg', 'bitmap_input', 'taken'), ('ret', 'cow_return', 'taken'))
                + G('usize', 'synthetic:0115-2|*', ('scope', 'bounded_readback+bitmap_input', 'entered'),
                    ('arg', 'bitmap_input', 'taken'), ('ret', 'bounded_readback', 'taken')),
                'bitmap input (arg) with a Cow return, and with a proven usize return, on one callable'))
    # O2: one method cross-listed under two snapshot families. The second
    # family's pair gate checks the method's own shape and refuses it; where
    # the first family precedes it in PAIR_POST, the first's state has been
    # collected before the refusal on the same pair; where it follows, it is
    # never collected.
    comp = [
        ('o2_array_then_indexed', 'array_snapshots', 'polars_core:3510', 'downcast_as_array', 'indexed_chunk_snapshots', 'array_snapshot', False, 'indexed_chunk'),
        ('o2_array_then_chunk', 'array_snapshots', 'polars_core:3510', 'downcast_as_array', 'chunk_snapshots', 'array_snapshot', False, 'chunk_snapshot'),
        ('o2_iter_then_array', 'iter_snapshots', 'polars_core:3505', 'downcast_iter', 'array_snapshots', 'iter_snapshot', True, 'array_snapshot'),
        ('o2_view_then_iter', 'view_snapshots', 'polars_core:3508', 'downcast_chunks', 'iter_snapshots', 'view_snapshot', True, 'iter_snapshot'),
        ('o2_owned_then_view', 'owned_iter_snapshots', 'polars_core:3504', 'downcast_into_iter', 'view_snapshots', 'owned_iter', True, 'view_snapshot'),
    ]
    for name, table, key, method, second, first, collected, refused in comp:
        t = text + add(second, key, f'{CA}::{method}', pairs_of(text, table, key))
        pat = f'{key}|*Int64*'
        if collected:
            expect = G('meet', pat, ('pair', first, 'active'), ('pair', refused, 'refused'))
        else:
            expect = G('meet', pat, ('pair', refused, 'refused')) + G('!first', pat, ('pair', first, 'active'))
        out.append((name, t, inv, expect,
                    f'{method} listed as {table} and {second}: the second pair gate refuses the method on every pair'))
    # O3: a later refusal after earlier state, on one pair
    t = text + add('array_snapshots', 'polars_core:3681', f'{CA}::chunks',
                   '[["polars_core::datatypes::Int64Type", "i64"]]')
    out.append(('o3_pair_refusal_after_state', t, inv,
                G('meet', 'polars_core:3681|*Int64*', ('pair', 'chunk_snapshot', 'active'), ('pair', 'array_snapshot', 'refused')),
                'chunks listed as a chunk snapshot (all pairs) and an array snapshot (Int64 only)'))
    t = text + f'\n[[hash_tokens]]\nkey = "polars_core:3434"\npath = "{CA}::rechunk"\ndirection = "sideways"\nsource = "u64"\ncite = "record 0115 overlap control"\n'
    out.append(('o3_callable_refusal_after_state', t, inv,
                G('meet', 'polars_core:3434|*', ('listed', 'cow_return', 'active'), ('listed', 'hash_token', 'refused')),
                'rechunk listed as a Cow return and as a malformed hash token'))
    # O4: a rejected pair followed by a valid pair of the same callable
    t = edit_block(text, 'chunk_snapshots', 'polars_core:3681',
                   lambda b: re.sub(r'(?ms)^pairs = \[.*?\]\]\s*$', 'pairs = [["polars_core::datatypes::Int64Type", "i64"]]\n', b))
    out.append(('o4_rejected_then_valid_pair', t, inv,
                G('rejected', 'polars_core:3681|*Int32*', ('pair', 'chunk_snapshot', 'refused'))
                + G('valid', 'polars_core:3681|*Int64*', ('pair', 'chunk_snapshot', 'active'), ('ret', 'chunk_snapshot', 'taken')),
                'chunks listed for Int64 only: Int32 refused, Int64 generated'))
    # O5: a listed callable followed by an unlisted one on the same path
    t = drop_block(text, 'cow_returns', 'polars_core:3641')
    out.append(('o5_listed_then_unlisted', t, inv,
                G('listed', 'polars_core:3611|*', ('listed', 'cow_return', 'active'), ('ret', 'cow_return', 'taken'))
                + G('!unlisted', 'polars_core:3641|*', ('listed', 'cow_return', 'active'))
                + G('!unlisted-ret', 'polars_core:3641|*', ('ret', 'cow_return', 'taken')),
                'to_physical_repr 3611 listed, 3641 (the same path, next) not'))
    # G: family refusals no generating run reaches
    t = edit_block(text, 'null_aware_returns', 'polars_core:3658',
                   lambda b: b.replace(', "polars_core::datatypes::Float64Type"', '', 1))
    out.append(('g_null_aware_pair_refused', t, inv, G('refused', 'polars_core:3658|*Float64*', ('pair', 'null_aware', 'refused')),
                'Float64 dropped from the null-aware types'))
    t = edit_block(text, 'method_scalar_generics', 'polars_core:3719',
                   lambda b: b.replace(', "polars_core::datatypes::Float64Type"', '', 1).replace(', "f64"', '', 1))
    out.append(('g_scalar_generic_pair_refused', t, inv, G('refused', 'polars_core:3719|*Float64*', ('pair', 'scalar_generic', 'refused')),
                'Float64 dropped from lhs_sub'))
    t = edit_block(text, 'sized_self_methods', 'polars_core:3499', lambda b: b.replace('params = ["usize"]', 'params = ["u64"]', 1))
    out.append(('g_sized_self_upstream', t, inv,
                G('!gate', 'polars_core:3499|*', ('pair', 'sized_self', 'active')) + G('!gate-refused', 'polars_core:3499|*', ('pair', 'sized_self', 'refused')),
                'limit listed with the wrong parameter: the census refuses it (unresolved) before any pair gate; the gate itself is asserted by unreachable_refusals_are_named'))
    t = edit_block(text, 'free_instantiations', 'polars_ops:1057', lambda b: b.replace('guard = "below_idx_max"', 'guard = "bogus"', 1))
    out.append(('g_arg_guard_upstream', t, inv,
                G('!guard', 'polars_ops:1057|*', ('free', 'arg_guard', 'active')) + G('!arg', 'polars_ops:1057|*', ('arg', 'arg_guard', 'refused')),
                'the index guard renamed to an unknown check: the guard shape refuses it before the arg hook; the hook itself is asserted by unreachable_refusals_are_named'))
    return out


def check(case_dir, trace_path):
    """Every positive group met on one concrete target; no negative line present."""
    import collections, fnmatch
    lines = set()
    for l in open(trace_path).read().splitlines():
        h, f, o, tgt = (l.split('\t') + [''])[:4]
        lines.add((h, f, o, tgt))
    groups = collections.OrderedDict()
    for l in open(os.path.join(case_dir, 'expect.tsv')).read().splitlines():
        g, h, f, o, pat = l.split('\t')
        groups.setdefault(g, []).append((h, f, o, pat))
    bad = []
    for g, want in groups.items():
        pat = want[0][3]
        assert all(w[3] == pat for w in want), g
        if g.startswith('!'):
            hit = sorted(tg for (h, f, o, tg) in lines for (wh, wf, wo, _) in want if (h, f, o) == (wh, wf, wo) and fnmatch.fnmatchcase(tg, pat))
            if hit:
                bad.append(f'{g}: present on {hit[0]}')
            else:
                print(f'  {g}: absent on {pat}')
            continue
        targets = sorted({tg for (_, _, _, tg) in lines if fnmatch.fnmatchcase(tg, pat)})
        met = [tg for tg in targets if all((h, f, o, tg) in lines for (h, f, o, _) in want)]
        if met:
            print(f'  {g}: met on {met[0]}' + (f' (and {len(met) - 1} more)' if len(met) > 1 else ''))
        else:
            bad.append(f'{g}: no single target matching {pat} carries {[w[:3] for w in want]}')
    for b in bad:
        print('  NOT MET: ' + b)
    return not bad


def write(outdir):
    text = open(RELEASE).read()
    inv = json.load(open(INVENTORY))
    manifest = []
    for name, t, i, expect, why in cases(text, inv):
        d = os.path.join(outdir, name)
        os.makedirs(d, exist_ok=True)
        open(os.path.join(d, 'release.toml'), 'w').write(t)
        if i is inv:
            p = os.path.join(d, 'inventory.json')
            if os.path.lexists(p):
                os.remove(p)
            os.symlink(INVENTORY, p)
        else:
            json.dump(i, open(os.path.join(d, 'inventory.json'), 'w'), sort_keys=True)
        open(os.path.join(d, 'expect.tsv'), 'w').write(''.join('\t'.join(e) + '\n' for e in expect))
        manifest.append(f'{name}\t{why}')
    open(os.path.join(outdir, 'cases.tsv'), 'w').write('\n'.join(manifest) + '\n')
    print(len(manifest), 'cases')


def self_test():
    t = '[[a]]\nkey = "k1"\npairs = [["x", "y"]]\n\n[[a]]\nkey = "k2"\npairs = [["z", "w"]]\n'
    assert pairs_of(t, 'a', 'k2') == '[["z", "w"]]'
    assert 'k1' not in drop_block(t, 'a', 'k1') and 'k2' in drop_block(t, 'a', 'k1')
    print('overlap self-test: ok')


if __name__ == '__main__':
    if sys.argv[1:] == ['--self-test']:
        self_test()
    elif sys.argv[1] == '--check':
        sys.exit(0 if check(sys.argv[2], sys.argv[3]) else 1)
    else:
        write(sys.argv[1])
