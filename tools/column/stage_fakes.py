"""Controller tests' fake stages: run in-process, in selection order."""
from contextlib import contextmanager
from unittest.mock import patch


@contextmanager
def inline(final):
	"""Every fake stage shares one resource, as the serial controller ran them."""
	import stage_scheduler
	with patch.object(final, 'STAGE_RUNNER', stage_scheduler.Inline), \
			patch.object(final.scheduler, 'resources', lambda *a, **k: frozenset({'fake'})):
		yield
