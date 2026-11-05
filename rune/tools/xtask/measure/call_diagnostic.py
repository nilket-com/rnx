"""Native-on interpreter-only Call diagnostic. Raw costs, no qualifications."""
import callgrind


def assess(rows):
    expected = {(n, role) for n in (1000, 2000) for role in ('tip', 'candidate')}
    indexed = {}
    for row in rows:
        key = row['invocations'], row['role']
        if (key not in expected or key in indexed or row['native'] != 'on'
                or row['native_entries'] != 0 or row['kernel'] != 'interpreter_call'):
            raise ValueError('foreign, duplicate or native-entered Call diagnostic')
        indexed[key] = row
    if indexed.keys() != expected:
        raise ValueError('incomplete Call diagnostic')
    deltas = {}
    for n in (1000, 2000):
        tip, candidate = (indexed[n, role] for role in ('tip', 'candidate'))
        if tip['checksum'] != candidate['checksum']:
            raise ValueError('Call checksum mismatch')
        costs = [callgrind.core_total(row['profile']) for row in (tip, candidate)]
        if any(type(cost) is not int or cost <= 0 for cost in costs):
            raise ValueError('missing interpreter core cost')
        deltas[n] = costs[1] - costs[0]
    growth = deltas[2000] - deltas[1000]
    return dict(status='PASS' if growth <= 0 else 'FAIL', deltas=deltas,
                incremental_growth_per_extra_1000=growth,
                cold_setup_delta=2*deltas[1000]-deltas[2000],
                instructions_per_extra_call=growth / 1000,
                scope='Native-on interpreter-only Call, raw profiles unchanged; '
                      'not a qualification or substitute for the CGU1 gates')
