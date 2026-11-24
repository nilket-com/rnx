import gdb
import json
import os
import struct
from pathlib import Path


def arm_inputs(config, state):
	method = config['meter']
	state['meter'] = method
	counters = state['input_threads'] = {}
	clock = state['clock'] = {'reads': 0, 'phases': {}}
	image = config.get('engine_binary', config['binary'])
	solib = '/proc/self/fd/9' if 'installed_binary' in config else config.get('engine_binary')
	product = Path(image).stat()
	seed = struct.pack('<QQ', *method['hash_keys'])
	pending = set()

	def observe_owner(frame):
		pc = frame.pc()
		if gdb.solib_name(pc) != solib:
			raise ValueError('input caller belongs to the wrong image')
		maps = Path('/proc', os.environ['COLUMN_CLIENT_PID'], 'maps')
		for row in maps.read_text().splitlines():
			span, perms, _, device, inode, *_ = row.split()
			start, end = [int(n, 16) for n in span.split('-')]
			if start <= pc < end and 'x' in perms:
				major, minor = [int(n, 16) for n in device.split(':')]
				if (os.makedev(major, minor), int(inode)) == (product.st_dev, product.st_ino):
					return
		raise ValueError('input caller is outside the pinned text mapping')

	class Returned(gdb.FinishBreakpoint):
		def __init__(self, frame, destination, kind, thread, counts):
			super().__init__(frame, internal=True)
			self.destination, self.kind, self.owner, self.counts = destination, kind, thread, counts

		def stop(self):
			try:
				expected = 16 if self.kind == 'seed' else 0
				frame = gdb.newest_frame()
				if gdb.selected_thread().ptid != self.owner or int(frame.read_register('rax')) != expected:
					raise ValueError('input return belongs to another thread or failed')
				observe_owner(frame)
				if self.kind == 'seed':
					value = seed
				else:
					tick = clock['reads']
					ns = method['epoch_ns'] + tick * method['tick_ns']
					value = struct.pack('<qq', *divmod(ns, 10**9))
					phase = ('after' if state['returns'] else 'execution' if 'split' in state
						else 'setup' if state['entries'] else 'before')
					period = clock['phases'].setdefault(phase, {'first': ns, 'count': 0})
					period['count'] += 1
					period['last'] = ns
					clock['reads'] += 1
				guest = gdb.selected_inferior()
				guest.write_memory(self.destination, value)
				if bytes(guest.read_memory(self.destination, len(value))) != value:
					raise ValueError('guest input was not installed')
				self.counts[self.kind + '_returns'] += 1
				pending.remove((self.owner, self.kind))
			except Exception as error:
				state['errors'].append(str(error))
			return False

		def out_of_scope(self):
			state['errors'].append('guest input call left its frame')

	class Input(gdb.Breakpoint):
		def __init__(self, symbol, kind):
			super().__init__(symbol, internal=True)
			self.kind = kind

		def stop(self):
			try:
				frame = gdb.newest_frame()
				caller = frame.older()
				if caller is None:
					raise ValueError('missing input caller')
				name = caller.name() or ''
				if self.kind == 'seed':
					if name != 'std::sys::random::linux::getrandom':
						return False
					init = caller.older()
					if (not init or 'RandomState>::new::KEYS::__rust_std_internal_init_fn' not in (init.name() or '')
							or int(frame.read_register('rsi')) != 16 or int(frame.read_register('rdx')) != 4):
						raise ValueError('unrecognized Rust seed initialization')
					observe_owner(init)
					buffer = int(frame.read_register('rdi'))
				else:
					if name != '<std::sys::pal::unix::time::Timespec>::now':
						if state['entries'] and not state['returns']:
							raise ValueError('unrecognized in-window clock caller')
						return False
					if int(frame.read_register('rdi')) != 1:
						raise ValueError('unreviewed clock domain')
					buffer = int(frame.read_register('rsi'))
				observe_owner(caller)
				thread = gdb.selected_thread().ptid
				key = (thread, self.kind)
				row = counters.setdefault(str(thread), dict(image=image, seed_entries=0,
					seed_returns=0, clock_entries=0, clock_returns=0))
				if key in pending or (self.kind == 'seed' and row['seed_entries']):
					raise ValueError('unexpected overlapping or repeated input initialization')
				pending.add(key)
				row[self.kind + '_entries'] += 1
				Returned(frame, buffer, self.kind, thread, row)
			except Exception as error:
				state['errors'].append(str(error))
			return False

	gdb.execute('set breakpoint pending on')
	Input('getrandom', 'seed')
	Input('clock_gettime', 'clock')

	def finish():
		if pending or sum(r['seed_returns'] for r in counters.values()) < 1:
			raise ValueError('missing or unfinished RandomState initialization')
		tick = method['tick_ns']
		next_ns = method['epoch_ns']
		for phase in ('before', 'setup', 'execution', 'after'):
			period = clock['phases'].get(phase)
			if period is None:
				continue
			if period['first'] != next_ns or period['last'] != next_ns + tick * (period['count'] - 1):
				raise ValueError('monotonic timeline reset or skipped a guest read')
			next_ns = period['last'] + tick
		if (not clock['reads'] or clock['reads'] != sum(r['clock_returns'] for r in counters.values())) or any(r['seed_entries'] != r['seed_returns'] or
				r['clock_entries'] != r['clock_returns'] for r in counters.values()):
			raise ValueError('guest input entry/return counts do not reconcile')
	return finish


config=json.loads(Path(os.environ['COLUMN_BOUNDARY_CONFIG']).read_text())
state={'status':'FAIL','entries':0,'returns':0,'errors':[],'armed':False}
phases=config.get('phase_symbols')
image = config.get('engine_binary')
installed = 'installed_binary' in config
solib = '/proc/self/fd/9' if installed else image
finish_inputs = None
if 'meter' in config:
	finish_inputs = arm_inputs(config, state)
if image:
	gdb.execute('tbreak main')
	gdb.execute('continue')
	gdb.execute('sharedlibrary ' + ('/proc/self/fd/9' if installed else 'libengine_image'))
	mapped = Path(image).stat()
	segments = []
	for line in Path('/proc', os.environ['COLUMN_CLIENT_PID'], 'maps').read_text().splitlines():
		span, perms, _, device, inode, *name = line.split(maxsplit=5)
		if name in ([image], [solib]) and 'x' in perms:
			major, minor = (int(n, 16) for n in device.split(':'))
			if mapped.st_dev != os.makedev(major, minor) or mapped.st_ino != int(inode):
				raise RuntimeError('mapped engine identity mismatch')
			segments.append([int(n, 16) for n in span.split('-')])
	if not segments:
		raise RuntimeError('bound engine has no executable mapping')
	state['engine'] = dict(artifact=config['measured']['engine'], text=segments)
def image_pc(pc):
	if not any(start <= pc < end for start, end in segments) or gdb.solib_name(pc) not in (image, solib):
		raise RuntimeError('selector outside bound engine text')
	return pc
def location(symbol):
	expression = "*'" + symbol + "'"
	if image:
		pc = image_pc(int(gdb.parse_and_eval("&'" + symbol + "'")))
		return '*' + hex(pc)
	return expression
outer=None
owner=None
caller_sp=None
def same_thread():
	return gdb.selected_thread().ptid==owner
def save():

	Path(config['gdb_receipt']).write_text(json.dumps(state,indent=2)+'\n')
class Return(gdb.FinishBreakpoint):
	def stop(self):
		try:
			if phases and (state.get('split') is None or not same_thread()):
				raise RuntimeError('return without an in-frame phase split')
			if image:
				frame = gdb.newest_frame()
				if not same_thread() or frame.pc() != state['caller_pc'] or frame.read_register('sp') != caller_sp:
					raise RuntimeError('finish did not reach the saved caller')
				state['return_pc'] = frame.pc()
			gdb.execute('monitor dump execution' if phases else 'monitor dump old-path')
			gdb.execute('monitor instrumentation off')
			state['returns']+=1
			save()
			return False
		except BaseException as e:
			state['errors'].append(repr(e));save();return True
	def out_of_scope(self):
		state['errors'].append('execution returned outside frame');save()
class Split(gdb.Breakpoint):
	def __init__(self, name, symbol):
		super().__init__(location(symbol),internal=True)
		self.expected=name
	def stop(self):
		try:
			frame=gdb.newest_frame()
			if (frame.name()!=self.expected or state['entries']!=1 or state['returns'] or
					'split' in state or not same_thread()):
				raise RuntimeError('wrong or repeated invocation phase split')
			if image:
				state['split_pc'] = image_pc(frame.pc())
			frame=frame.older()
			while frame is not None and frame!=outer:
				frame=frame.older()
			if frame is None:
				raise RuntimeError('split outside the measured invocation')
			gdb.execute('monitor dump setup')
			state['split']=self.expected
			return False
		except BaseException as e:
			state['errors'].append(repr(e));save();return True
class Entry(gdb.Breakpoint):

	def stop(self):
		try:
			global outer,owner,caller_sp
			if phases or image:
				if state['entries']:
					raise RuntimeError('nested invocation')
				outer=gdb.newest_frame();owner=gdb.selected_thread().ptid
				state['thread']=owner
			else:
				self.enabled=False
			if image:
				state['entry_pc'] = image_pc(outer.pc())
				caller = outer.older()
				state['caller_pc'] = caller.pc()
				caller_sp = int(caller.read_register('sp'))
			state['entries']+=1
			Return(gdb.newest_frame(),internal=True)
			gdb.execute('monitor instrumentation on')
			gdb.execute('monitor zero')
			return False
		except BaseException as e:
			state['errors'].append(repr(e));save();return True

def exited(event):
	if finish_inputs:
		try:
			finish_inputs()
		except ValueError as error:
			state['errors'].append(str(error))
	state['exit']=event.exit_code
	if event.exit_code==0 and state['armed'] and state['entries']==state['returns']==1 and not state['errors']:state['status']='PASS'
	save()
gdb.events.exited.connect(exited)
entry=Entry(location(config['entry']),internal=True)
if config.get('arm'):
	entry.enabled=False
	class Arm(gdb.Breakpoint):
		def stop(self):
			self.enabled=False
			state['armed']=True
			entry.enabled=True
			return False
	Arm("*'"+config['arm']+"'",internal=True)
else:
	state['armed']=True

if phases:
	for name,symbol in phases.items():
		Split(name,symbol)
