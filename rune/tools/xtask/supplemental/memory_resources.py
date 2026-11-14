"""Conservative cache-aware admission observations; no kernel reclaim writes."""
import os
from pathlib import Path


class ResourceObservationError(ValueError):
	pass


class InsufficientMemory(ValueError):
	def __init__(self, observation, reserve, per_worker):
		self.numbers = dict(host_available_bytes=observation['mem_available_bytes'],
			cgroup_headroom_bytes=observation['ancestor_headroom_bytes'],
			binding_cgroup=observation['binding_cgroup'], binding_limit=observation['binding_limit'],
			reserve_bytes=reserve, per_worker_bytes=per_worker, workers=0)
		super().__init__('InsufficientMemory: ' + ', '.join(f'{k}={v}' for k, v in self.numbers.items()))


def number(text):
	if not text.isascii() or not text.isdecimal():
		raise ResourceObservationError('nonnegative decimal accounting value required')
	return int(text)


def observe(extra_group=None, *, root=Path('/sys/fs/cgroup'), meminfo=Path('/proc/meminfo'),
		membership=Path('/proc/self/cgroup'), cpus=None):
	"""All finite high/max limits bind, including the destination's ancestors."""
	try:
		cpu_ids = sorted(os.sched_getaffinity(0) if cpus is None else cpus)
		if not cpu_ids: raise ResourceObservationError('empty CPU affinity')
		mem = dict(line.split(':', 1) for line in meminfo.read_text().splitlines())
		amount, unit = mem['MemAvailable'].split()
		if unit != 'kB': raise ResourceObservationError('unknown MemAvailable unit')
		available = number(amount) * 1024
		unified = [line.split(':', 2)[2] for line in membership.read_text().splitlines() if line.startswith('0::')]
		if len(unified) != 1: raise ResourceObservationError('unified membership missing or ambiguous')
		result = dict(cpu_ids=cpu_ids, mem_available_bytes=available, ancestor_headroom_bytes=available,
			binding_cgroup='host', binding_limit='MemAvailable', groups=[])
		seen = set()
		for name in [unified[0]] + ([extra_group] if extra_group is not None else []):
			if not name.startswith('/') or '..' in Path(name).parts:
				raise ResourceObservationError('invalid control-group path')
			group = root / name.lstrip('/')
			while group != root:
				if group in seen: break
				seen.add(group)
				if group.resolve() != group.absolute(): raise ResourceObservationError('linked control group')
				current = number((group/'memory.current').read_text().strip())
				stats = {}
				for line in (group/'memory.stat').read_text().splitlines():
					key, value = line.split()
					if key in stats: raise ResourceObservationError('duplicate memory.stat key')
					stats[key] = number(value)
				fields = {key: stats[key] for key in ('file', 'shmem', 'inactive_file', 'file_dirty', 'file_writeback', 'file_mapped')}
				credit = min(current, max(0, min(fields['inactive_file'], max(0, fields['file']-fields['shmem']))
					-fields['file_dirty']-fields['file_writeback']-fields['file_mapped']))
				row = dict(group='/' + str(group.relative_to(root)), current_bytes=current,
					fields=fields, cache_credit_bytes=credit, limits={})
				for key in ('memory.max', 'memory.high'):
					text = (group/key).read_text().strip()
					if text == 'max':
						row['limits'][key] = None
						continue
					limit = number(text)
					usable = max(0, limit-current+credit)
					row['limits'][key] = dict(limit_bytes=limit, raw_headroom_bytes=max(0, limit-current), usable_bytes=usable)
					if usable < result['ancestor_headroom_bytes']:
						result.update(ancestor_headroom_bytes=usable, binding_cgroup=row['group'], binding_limit=key)
				result['groups'].append(row)
				group = group.parent
		return result
	except (OSError, KeyError, ValueError, StopIteration) as error:
		if isinstance(error, ResourceObservationError): raise
		raise ResourceObservationError('cannot read admission accounting: ' + str(error)) from error


def capacity(cpus, available, headroom, reserve, per_worker, *, requested=None, observation=None):
	if any(type(v) is not int or v < 0 for v in (available, headroom, reserve)) or type(cpus) is not int or cpus <= 0 or type(per_worker) is not int or per_worker <= 0:
		raise ResourceObservationError('invalid resource observation or reservation')
	if requested is not None and (type(requested) is not int or requested <= 0):
		raise ValueError('positive worker override required')
	ceiling = min(available, headroom)-reserve
	workers = min(cpus, max(0, ceiling//per_worker))
	if requested is not None: workers = min(workers, requested)
	if workers < 1:
		raise InsufficientMemory(observation or dict(mem_available_bytes=available,
			ancestor_headroom_bytes=headroom, binding_cgroup='provided headroom', binding_limit='provided'), reserve, per_worker)
	return workers, ceiling
