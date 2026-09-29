#!/usr/bin/env python3
"""Record 0116: the final disposition of each of the 793 `generic_methods`
rows, from the finished v2 surface, plus rule 5's per-cause census of the
chunked pool.

Each row keeps its plan family (partition.sh's audit) and gets exactly one
final disposition:
  generated                  bound; with its binding count
  refused                    the generator's named reason (rules 1-3, 6, 7)
  stopped:trait_arguments    I/O: constructed and finished only through the
                             generic traits SerReader<R> / SerWriter<W>
                             (trait-argument modelling, its own record)
  census                     chunked_pairs: census only in this record
  deferred:<rule>            the batch table's owning rule, with its cause
  refused:<cause>            a named refusal this record owns (audit.py)

  final.py <partition audit.json> <final surface-v2.json> <out.json>
"""
import collections, gzip, json, re, sys


def load(p):
    return json.load(gzip.open(p) if p.endswith('.gz') else open(p))


# rule 3's reachability (dtype owners not listed, and why)
DTYPE_REASON = {
    'PrimitiveChunkedBuilder': 'no script path to a result: append/finish are ChunkedBuilder<N, T> generic-trait methods (trait-argument modelling)',
    'CategoricalChunkedBuilder': 'no script path to a result: finish returns CategoricalChunked<T>, an alias that is itself generic',
    'ObjectChunkedBuilder': 'T: PolarsObject has no script object type',
    'ObjectArray': 'T: PolarsObject has no script object type',
}
OTHER_OWNER_REASON = {
    'DatetimeInfer': 'no script-reachable constructor (listed instantiations would be unreachable)',
    'NoNull': 'a newtype over any T: no dtype table',
    'SpecialEq': 'a newtype over any T: no dtype table',
    'BaseColumnUdf': 'deferred:callbacks',
}


def main(ap, sp, out):
    part = load(ap)['rows']
    surf = load(sp)
    E = {e['key']: e for e in surf['entries']}
    pairs = collections.defaultdict(list)
    for p in surf['instantiation']['pairs']:
        pairs[p['key']].append(p)
    rows = []
    for r in part:
        e = E[r['key']]
        fam = r['family']
        owner = r['owner'].rsplit('::', 1)[-1]
        d = {'key': r['key'], 'path': r['path'], 'family': fam}
        if e['status'] == 'generated':
            d.update(disposition='generated', bindings=len(e.get('bindings') or []) or 1)
        elif fam == 'io_generic':
            d.update(disposition='stopped:trait_arguments',
                     reason='constructed and finished only through SerReader<R>/SerWriter<W>, generic traits whose argument is the instantiated source/sink; trait-argument modelling is its own record')
        elif fam == 'chunked_pairs':
            causes = collections.Counter(
                re.sub(r'`[^`]*`', '`X`', (p.get('disposition') or '')).split(':')[0] + ': ' +
                re.sub(r'`[^`]*`', '`X`', (p.get('disposition') or '')).split(':', 1)[-1].strip()[:70]
                for p in pairs.get(r['key'], []))
            d.update(disposition='census', reason=e.get('reason') or '', pair_causes=dict(causes))
        elif fam == 'dtype_owner' and owner in DTYPE_REASON:
            d.update(disposition='refused', reason=DTYPE_REASON[owner])
        elif fam == 'generic_owner_other' and owner in OTHER_OWNER_REASON:
            why = OTHER_OWNER_REASON[owner]
            d.update(disposition=why if why.startswith('deferred:') else 'refused', reason=why)
        elif fam.startswith(('defer:', 'refused:')):
            d.update(disposition=fam.replace('defer:', 'deferred:'), reason=r['reason'])
        else:
            d.update(disposition='refused', reason=e.get('reason') or '')
        assert d['disposition'] != 'refused' or d['reason'], d
        rows.append(d)
    assert len(rows) == 793 and len({r['key'] for r in rows}) == 793
    by = collections.Counter((r['family'], r['disposition'].split(':')[0] if not r['disposition'].startswith(('deferred', 'refused:', 'stopped')) else r['disposition']) for r in rows)
    gen = sum(1 for r in rows if r['disposition'] == 'generated')
    census = collections.Counter()
    for r in rows:
        for c, n in r.get('pair_causes', {}).items():
            census[c] += n
    json.dump({'rows': rows, 'counts': {f'{a} | {b}': n for (a, b), n in sorted(by.items())},
               'generated': gen, 'bindings': sum(r.get('bindings', 0) for r in rows),
               'chunked_census': dict(census.most_common())}, open(out, 'w'), indent=1)
    for (a, b), n in sorted(by.items()):
        print(f'{n:4}  {a:22} {b}')
    print(f'generated rows: {gen}; chunked pair causes: {len(census)}')


if __name__ == '__main__':
    main(*sys.argv[1:4])
