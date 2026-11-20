import gdb
import json
import os
from pathlib import Path
config=json.loads(Path(os.environ['COLUMN_BOUNDARY_CONFIG']).read_text())
state={'status':'FAIL','entries':0,'returns':0,'errors':[],'armed':False}
phases=config.get('phase_symbols')
image = config.get('engine_binary')
if image:
	gdb.execute('tbreak main')
	gdb.execute('continue')
	gdb.execute('sharedlibrary libengine_image')
	mapped = Path(image).stat()
	segments = []
	for line in Path('/proc', os.environ['COLUMN_CLIENT_PID'], 'maps').read_text().splitlines():
		span, perms, _, device, inode, *name = line.split(maxsplit=5)
		if name == [image] and 'x' in perms:
			major, minor = (int(n, 16) for n in device.split(':'))
			if mapped.st_dev != os.makedev(major, minor) or mapped.st_ino != int(inode):
				raise RuntimeError('mapped engine identity mismatch')
			segments.append([int(n, 16) for n in span.split('-')])
	if not segments:
		raise RuntimeError('bound engine has no executable mapping')
	state['engine'] = dict(artifact=config['measured']['engine'], text=segments)
def image_pc(pc):
	if not any(start <= pc < end for start, end in segments) or gdb.solib_name(pc) != image:
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
