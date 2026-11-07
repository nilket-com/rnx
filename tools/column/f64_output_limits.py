"""Whole-map ceilings and system bands; instruction totals are never adjusted."""
import json
from pathlib import Path
from ir_accounting import system_band


def limits(index, kinds, layouts, sizes):
	policy = json.loads(Path(__file__).with_name('CK2.json').read_text())
	failures = []
	for kind in kinds:
		for layout in layouts:
			for call in range(2):
				system = []
				for rows in sizes:
					a = index[kind, layout, rows, 'baseline']['costs'][call]
					b = index[kind, layout, rows, 'candidate']['costs'][call]
					for cost in (a, b):
						if type(cost['Ir']) is not int or cost['Ir'] <= 0 or cost['Ir_per_row'] != cost['Ir'] / rows:
							raise ValueError('invalid whole-map instruction total')
					system.append(b['accounting']['dynamic_Ir'] - a['accounting']['dynamic_Ir'])
					cap = None
					if kind == 'f64chain':
						cap = 22 if layout in ('flat', 'sliced-chunked-null') else 30
						if b['Ir'] > a['Ir']:
							failures.append(dict(rule='F64 whole-map nonpositive', kind=kind, layout=layout, rows=rows, call=call))
					elif kind in ('booland', 'boolor', 'boolchain'):
						cap = (2.5 if layout == 'flat' else 5) if kind == 'boolchain' else (2 if layout == 'flat' else 4)
					elif kind in ('i64add', 'axpy', 'f64const', 'f64add', 'multiply3', 'boolnot'):
						if layout == 'flat':
							cap = policy['flat_ir_per_row_max'].get('mul3' if kind == 'multiply3' else kind)
						else:
							flat = index[kind, 'flat', rows, 'candidate']['costs'][call]['Ir'] / rows
							cap = policy['nullable_boolnot_ir_per_row_max'] if kind == 'boolnot' else policy['nullable_numeric_ratio_to_new_flat_max'] * flat
					if cap is not None and b['Ir'] / rows > cap:
						failures.append(dict(rule='whole-map ceiling', kind=kind, layout=layout, rows=rows, call=call, actual=b['Ir'] / rows, ceiling=cap))
				if kind != 'f64chain':
					band = system_band(*system, *sizes)
					if not band['passes']:
						failures.append(dict(rule='unchanged system band', kind=kind, layout=layout, call=call, **band))
	return failures
