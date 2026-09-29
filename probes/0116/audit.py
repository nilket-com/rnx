#!/usr/bin/env python3
"""Record 0116 (plan evidence): the 793 rows of the 0113 batch table's
`generic_methods` rule, each assigned one family by the refusal the v2
generator gives it once the 0072 `generic` bucket gate is lifted (the
generator's own mapping rules then decide, and fail closed with a reason).

  audit.py <0113 batches.json> <v2 inventory.json> <surface with generic admitted> <out.json>
"""
import collections, gzip, json, re, sys


def load(p):
    return json.load(gzip.open(p) if p.endswith('.gz') else open(p))


IO_BOUNDS = ('std::io::Read', 'std::io::Write', 'std::io::Seek', 'MmapBytesReader', 'io::read::Read',
             'io::write::Write', 'core::io', 'std::io')


def family(c, e, sup, aliases):
    r = e.get('reason') or ''
    owner = c['owner']
    if e['status'] == 'generated':
        return 'already', 'generated once the generic bucket gate is lifted'
    if r.startswith('generic owner') or r.startswith('lifetime owner') and owner.startswith('polars_io'):
        if owner in aliases:
            return 'alias_owner', f"{owner.rsplit('::', 1)[-1]} has concrete aliases {sorted(aliases[owner])}"
        if owner.startswith('polars_io::'):
            return 'io_generic', 'reader/writer generic over its source or sink'
        if '::builder::' in owner or owner.endswith('ObjectArray'):
            return 'dtype_owner', 'generic owner instantiable by the dtype table'
        return 'generic_owner_other', r
    if 'mutable reference: writer' in r or 'mutable reference: reader' in r:
        return 'io_generic', r
    if ('instantiation' in r or r.startswith('generic type: ca') or 'generic parameter not inferable' in r) and chunked(c, r):
        return 'chunked_pairs', r
    if 'chrono_tz' in r or 'Tz' in r and 'foreign type' in r:
        return 'time_zone', r
    if 'no wrapped implementor' in r or 'on every implementor' in r or 'trait has no public path' in r:
        return 'defer:trait_dispatch', r
    for pats, fam in DETAIL:
        if any(re.search(p, r) for p in pats):
            return fam, r
    if (r.startswith('generic type: ') or r.startswith('bucket: generic')) and chunked(c, r):
        return 'chunked_pairs', r
    raise SystemExit(f'unclassified row {c["key"]} {c["canonical_path"]}: {r}')


# Every remaining refusal, by its concrete cause, to the rule that owns it
# (the batch table's rules) or to a named refusal this record owns. A row
# matching none is an error: there is no catch-all.
DETAIL = [
    ((r'^async:',), 'defer:async'),
    ((r'PlHashMap<', r'PlHashSet<', r'PlIndexSet<', r'lifetime', r'chrono::naive', r'fn\(i64\) -> NaiveDateTime', r'either::Either', r'bitflags::iter',
      r'iter::adapters::zip', r'impl core::fmt::Display'), 'defer:value_grammar'),
    ((r'unreachable polars type', r'owner has no public path', r'lp_arena', r'object_store', r'jsonpath_lib',
      r'Arc<dyn polars_io::cloud', r'foreign type: builder', r'polars_arrow::datatypes', r'AtomicU64',
      r'no public path', r'LineStats; \.\.', r'polars_utils::arena::Arena', r'polars_utils::idx_vec::UnitVec',
      r'polars_arrow::array::', r'polars_compute::'), 'defer:internal_crates'),
    ((r'bound: (f|func|fun|closure)\b', r'not inferable from arguments: (V|T|N|I|Phantom)$', r'fn\(&str, PolarsWarning\)'),
     'defer:callbacks'),
    ((r'bound: (reader|writer) ', r'mutable reference: (writer|reader)', r'ByteSourceReader<'), 'io_generic'),
    ((r'Box<dyn ', r'Arc<dyn ', r'dyn polars', r'trait object', r'dyn core::any::Any'), 'refused:trait_object_value'),
    ((r'SeedableRandomState',), 'refused:hasher_state'),
    ((r'std::fs::File', r'std::path::PathBuf'), 'refused:file_system_boundary'),
    ((r"static borrow",), 'refused:static_borrow_parameter'),
    ((r'^name taken in polars:: by',), 'refused:name_taken_in_facade'),
    ((r'generic return: return \(never\)',), 'refused:never_return'),
    ((r'^receiver: self: ',), 'refused:receiver_shape'),
    ((r'^bound: (iter|par_iter|rhs|v|array|error|err) ', r'impl return: return'), 'refused:unbounded_generic_input'),
]


def chunked(c, r):
    """A chunked-array pair: a chunked owner, or a chunked parameter or return."""
    return bool(re.search(r'polars_core::chunked_array::(ChunkedArray|logical::Logical)\b', c['owner'])
                or c['owner'].startswith('polars_core::chunked_array::ops::')
                or re.search(r'ChunkedArray<|Logical<|Chunked\b', r))


def main(bp, ip, sp, out):
    keys = next(r for r in load(bp)['rules'] if r['rule'] == 'generic_methods')['keys']
    inv = load(ip)
    C = {c['key']: c for c in inv['callables']}
    sup = {s['canonical_path']: s for s in inv['supporting']}
    aliases = collections.defaultdict(set)
    for s in inv['supporting']:
        if s.get('alias_target'):
            aliases[s['alias_target'].split('<')[0]].add(s['canonical_path'].rsplit('::', 1)[-1])
    E = {e['key']: e for e in load(sp)['entries']}
    rows = []
    for k in keys:
        f, why = family(C[k], E[k], sup, aliases)
        rows.append({'key': k, 'path': C[k]['canonical_path'], 'owner': C[k]['owner'], 'family': f, 'reason': why})
    counts = collections.Counter(r['family'] for r in rows)
    json.dump({'rows': rows, 'counts': dict(sorted(counts.items()))}, open(out, 'w'), indent=1)
    for f, n in sorted(counts.items(), key=lambda x: -x[1]):
        print(f'{n:4} {f}')
    assert len(rows) == 793


if __name__ == '__main__':
    main(*sys.argv[1:5])
