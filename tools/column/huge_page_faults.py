"""ADR 0227 best-effort fault gate; retained captures are never retried."""
import itertools
import statistics


def evaluate(calls):
	expected = set(itertools.product(('f64const', 'f64add', 'f64chain'), ('flat', 'alternating'), range(3)))
	keys = [(c['kind'], c['layout'], c['call']) for c in calls]
	if len(keys) != len(expected) or set(keys) != expected:
		raise ValueError('missing, duplicate or unexpected fault call')
	for c in calls:
		if type(c['baseline_minor']) is not int or c['baseline_minor'] <= 0 or type(c['candidate_minor']) is not int or c['candidate_minor'] < 0:
			raise ValueError('invalid fault count')
	median = statistics.median(c['baseline_minor'] / max(c['candidate_minor'], 1) for c in calls)
	failures = [dict(kind=c['kind'], layout=c['layout'], call=c['call'], rule='faults exceed baseline') for c in calls if c['candidate_minor'] > c['baseline_minor']]
	if median < 10:
		failures.append(dict(rule='median reduction below 10x', actual=median))
	return dict(status='FAIL' if failures else 'PASS', median_reduction=median, failures=failures,
		policy='No call exceeds baseline faults; median reduction across all 18 calls at least 10x')
