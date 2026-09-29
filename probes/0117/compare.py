#!/usr/bin/env python3
"""Record 0117 stage A: the new inventory differs from the old only by
  - the new fields: `trait_params` (supporting), `trait_args` (trait impls
    and callables);
  - trait impl rows (and their implementor labels) that the corrected
    deduplication recovers, each listed.
Anything else is a failure.

  compare.py <old inventory.json> <new inventory.json> [--json out]
"""
import json, sys


def strip_sup(s):
    s = dict(s)
    s.pop('trait_params', None)
    s['impls'] = [{k: v for k, v in i.items() if k != 'trait_args'} for i in s.get('impls', [])]
    return s


def main(op, np, out=None):
    old, new = json.load(open(op)), json.load(open(np))
    problems, recovered = [], []
    for k in old:
        if k not in ('callables', 'supporting') and old[k] != new.get(k):
            problems.append(f'top-level {k} differs')
    oc = {c['key']: c for c in old['callables']}
    nc = {c['key']: c for c in new['callables']}
    if set(oc) != set(nc):
        problems.append(f'callable keys differ: -{sorted(set(oc) - set(nc))[:5]} +{sorted(set(nc) - set(oc))[:5]}')
    for k in oc.keys() & nc.keys():
        a, b = dict(oc[k]), dict(nc[k])
        b.pop('trait_args', None)
        ia, ib = a.pop('implementors', []), b.pop('implementors', [])
        if a != b:
            problems.append(f'callable {k} differs in {sorted(x for x in a if a.get(x) != b.get(x))}')
        # implementor labels: the old list must be a sub-multiset of the new
        rest = list(ib)
        for x in ia:
            if x in rest:
                rest.remove(x)
            else:
                problems.append(f'callable {k} lost implementor {x}')
        if rest:
            recovered.append({'callable': k, 'path': oc[k]['canonical_path'], 'added_implementors': rest})
    os_ = {s['key']: s for s in old['supporting']}
    ns = {s['key']: s for s in new['supporting']}
    if set(os_) != set(ns):
        problems.append('supporting keys differ')
    for k in sorted(os_.keys() & ns.keys()):
        a, b = strip_sup(os_[k]), strip_sup(ns[k])
        ia, ib = a.pop('impls'), b.pop('impls')
        la, lb = a.pop('implementors', []), b.pop('implementors', [])
        rest_l = list(lb)
        for x in la:
            if x in rest_l:
                rest_l.remove(x)
            else:
                problems.append(f'supporting {k} lost implementor {x}')
        if rest_l:
            recovered.append({'trait_labels': os_[k]['canonical_path'], 'added_implementors': rest_l})
        if a != b:
            problems.append(f'supporting {k} differs in {sorted(x for x in a if a.get(x) != b.get(x))}')
        rest = [json.dumps(i, sort_keys=True) for i in ib]
        for i in ia:
            j = json.dumps(i, sort_keys=True)
            if j in rest:
                rest.remove(j)
            else:
                problems.append(f'supporting {k} lost impl {i["for_type"]}')
        full = [i for i in ns[k]['impls']]
        for j in rest:
            i = json.loads(j)
            match = [f for f in full if {x: y for x, y in f.items() if x != 'trait_args'} == i]
            recovered.append({'trait': os_[k]['canonical_path'], 'for_type': i['for_type'],
                              'trait_args': match[0].get('trait_args') if match else None})
    recovered.sort(key=lambda r: json.dumps(r, sort_keys=True))
    doc = {'problems': problems, 'recovered': recovered,
           'recovered_impls': sum(1 for r in recovered if 'trait' in r),
           'callables_with_added_implementors': sum(1 for r in recovered if 'callable' in r),
           'trait_params_set': sum(1 for s in new['supporting'] if s.get('trait_params')),
           'impls_with_trait_args': sum(1 for s in new['supporting'] for i in s.get('impls', []) if i.get('trait_args'))}
    if out:
        json.dump(doc, open(out, 'w'), indent=1)
    print(json.dumps({k: v for k, v in doc.items() if k != 'recovered'}, indent=1))
    for r in recovered[:12]:
        print('  recovered:', r)
    sys.exit(1 if problems else 0)


if __name__ == '__main__':
    a = sys.argv[1:]
    main(a[0], a[1], a[3] if len(a) > 3 and a[2] == '--json' else None)
