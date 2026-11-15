#!/usr/bin/python3
"""Protocol fixture for the notebook application called by journey.rn; no real application or network."""
import json
from pathlib import Path
import sys

ROOT = Path('/work/run')
OUTPUT = ROOT / 'journey-output'
DIGEST = 'journey-fixture-digest'
KINDS = ('describe_notebook', 'run_cell', 'describe_notebook', 'run_cell', 'open_data',
         'describe_notebook', 'replace_cell', 'describe_notebook', 'run_cell',
         'describe_notebook', 'run_cell', 'describe_notebook', 'replace_cell',
         'describe_notebook', 'run_cell', 'describe_notebook', 'run_cell', 'describe', 'publish')


def expected(n):
    kind = KINDS[n - 1]
    target = dict(buffer=1, generation=1, cell=20 if n in (4, 11, 17) else 10)
    if kind in ('describe_notebook', 'describe'):
        body = {}
    elif kind == 'run_cell':
        body = dict(target=target)
    elif kind == 'open_data':
        body = dict(path=str(ROOT / 'fixture-data.csv'))
    elif kind == 'replace_cell':
        body = dict(target=target, source=f'SELECT "x" AS x, "y" * {1 if n == 7 else 2} AS y FROM "fixture_table";\n')
    elif kind == 'publish':
        body = dict(directory=str(OUTPUT / 'package'))
    else:
        raise ValueError('unexpected command kind ' + kind)
    return {kind: body}


def answer(n, command):
    if not 1 <= n <= len(KINDS) or command != expected(n):
        raise ValueError(f'unexpected command at action {n}: {command!r}')
    kind = KINDS[n - 1]
    if kind == 'describe_notebook':
        return dict(notebook=dict(buffer=1, generation=1, cells=[
            dict(cell=30), dict(name='data', cell=10), dict(name='plot', cell=20)]))
    if kind == 'run_cell':
        return dict(figures=[dict(figure=n)]) if n in (4, 11, 17) else {}
    if kind == 'open_data':
        return dict(data=dict(columns=[['x', 'i64'], ['y', 'f64'], ['label', 'String']], table='fixture_table'))
    if kind == 'publish':
        return dict(publish=dict(digest=DIGEST))
    return {}


def main():
    if len(sys.argv) != 5 or sys.argv[1:4] != ['act', 'fixture-window', '--commands']:
        raise ValueError('unexpected command invocation; expected act fixture-window --commands PATH')
    state = ROOT / 'journey-state.json'
    n = json.loads(state.read_text())['next'] if state.exists() else 1
    if not 1 <= n <= len(KINDS) or Path(sys.argv[4]) != OUTPUT / f'{n}-commands.json':
        raise ValueError(f'unexpected command file or action sequence at {n}')
    request = json.loads(Path(sys.argv[4]).read_text())
    if not isinstance(request, dict) or set(request) != {'commands'} or len(request['commands']) != 1:
        raise ValueError('unexpected command envelope')
    receipt = dict(outcome='applied', **answer(n, request['commands'][0]))
    state.write_text(json.dumps(dict(next=n + 1), sort_keys=True) + '\n')
    print(json.dumps(dict(event='irrelevant'), sort_keys=True))
    print(json.dumps(dict(id=2, event='irrelevant'), sort_keys=True))
    print(json.dumps(dict(id=1, event='applied', terminal=True, receipt=receipt), sort_keys=True))


if __name__ == '__main__':
    try:
        main()
    except (ValueError, KeyError, OSError, TypeError, IndexError) as error:
        print('journey protocol fixture: ' + str(error), file=sys.stderr)
        raise SystemExit(2)
