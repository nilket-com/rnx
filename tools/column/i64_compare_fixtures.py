"""ADR 0233: all six signed comparisons, pair and both constant orders."""
import operator

OPERATORS = {'eq': ('==', operator.eq), 'ne': ('!=', operator.ne),
	'lt': ('<', operator.lt), 'le': ('<=', operator.le),
	'gt': ('>', operator.gt), 'ge': ('>=', operator.ge)}


def specs():
	result = {}
	for name, (symbol, compare) in OPERATORS.items():
		for shape, params, expression, inputs, operands in (
			('pair', 'x,y', f'x {symbol} y', ['2', '3'], (2, 3)),
			('right', 'x', f'x {symbol} 1', ['2'], (2, 1)),
			('left', 'x', f'1 {symbol} x', ['2'], (1, 2)),
		):
			result[f'i64{name}_{shape}'] = ('i64', f'pub fn map({params}) {{ {expression} }}',
				inputs, str(compare(*operands)).lower(), ['i64'] * len(inputs))
	return result


def install(controller, fixtures):
	import predicate_fixtures
	predicate_fixtures.SPECS.update(specs())
	return predicate_fixtures.install(controller, fixtures)
