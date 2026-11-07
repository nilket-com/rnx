"""Reviewed whole-map targets; unchanged-path system bands remain in force."""
import itertools
from f64_output_limits import limits as prior_limits
from ir_accounting import system_band

CEILINGS = dict(f64gt=6, f64le=5.5, i64eq=10, f64and=11, mixedor=12, arithcmp=13.5)


def limits(index, protected, layouts, sizes):
	# ADR0223: f64chain is unchanged, so the ELF-object rule replaces only
	# ADR0222's changed-path whole-total nonpositive condition. Keep ceilings.
	failures = [f for f in prior_limits(index, protected, layouts, sizes)
		if not (f['rule'] == 'F64 whole-map nonpositive' and f['kind'] == 'f64chain')]
	# f64chain is now unchanged too; the earlier record excluded it only while
	# changing its algorithm. Keep the same system band for this protected path.
	for layout, call in itertools.product(layouts, range(2)):
		for rows in sizes:
			a, b = (index['f64chain',layout,rows,role]['costs'][call]['accounting'] for role in ('baseline','candidate'))
			if b['rnx_Ir'] > a['rnx_Ir'] or b['other_Ir'] != a['other_Ir']:
				failures.append(dict(rule='protected F64 program nonpositive / unknown exact',layout=layout,rows=rows,call=call))
		deltas = [index['f64chain',layout,n,'candidate']['costs'][call]['accounting']['dynamic_Ir'] - index['f64chain',layout,n,'baseline']['costs'][call]['accounting']['dynamic_Ir'] for n in sizes]
		band = system_band(*deltas, *sizes)
		if not band['passes']: failures.append(dict(rule='unchanged system band',kind='f64chain',layout=layout,call=call,**band))
	for kind, layout, rows, call in itertools.product(CEILINGS, layouts, sizes, range(2)):
		a, b = (index[kind,layout,rows,role]['costs'][call] for role in ('baseline','candidate'))
		for cost in (a,b):
			if type(cost['Ir']) is not int or cost['Ir'] <= 0 or cost['Ir_per_row'] != cost['Ir']/rows:
				raise ValueError('invalid whole-map total')
		case = dict(kind=kind,layout=layout,rows=rows,call=call)
		if b['Ir']/rows > CEILINGS[kind]: failures.append(dict(case,rule='predicate ceiling',actual=b['Ir']/rows,maximum=CEILINGS[kind]))
		if b['Ir']*4 > a['Ir']: failures.append(dict(case,rule='predicate baseline ratio',actual=b['Ir']/a['Ir'],maximum=0.25))
	return failures
