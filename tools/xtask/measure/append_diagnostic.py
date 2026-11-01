"""Interpreter-only append per-call diagnostic. No profile adjustment or exemption."""
import callgrind


def assess(rows):
    expected = {(n, role) for n in (1000, 2000) for role in ('tip', 'candidate')}
    indexed = {}
    for row in rows:
        key = row['invocations'], row['role']
        if key not in expected or key in indexed or row['native'] != 'off':
            raise ValueError('foreign, duplicate or native-enabled append diagnostic')
        indexed[key] = row
    if indexed.keys() != expected:
        raise ValueError('incomplete append diagnostic')
    deltas = {}
    for n in (1000, 2000):
        tip, candidate = (indexed[n, role] for role in ('tip', 'candidate'))
        if tip['checksum'] != candidate['checksum']:
            raise ValueError('append checksum mismatch')
        costs = [callgrind.core_total(row['profile']) for row in (tip, candidate)]
        if any(type(cost) is not int or cost <= 0 for cost in costs):
            raise ValueError('missing interpreter core cost')
        deltas[n] = costs[1] - costs[0]
    growth = deltas[2000] - deltas[1000]
    return dict(status='PASS' if growth <= 0 else 'FAIL', deltas=deltas,
                incremental_growth_per_extra_1000=growth,
                cold_setup_delta=2*deltas[1000]-deltas[2000],
                instructions_per_extra_call=growth / 1000,
                scope='Native-off append diagnostic only; raw profiles unchanged; '
                      'not a qualification or substitute for the CGU1 gates')
