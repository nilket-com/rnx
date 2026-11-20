import gdb
import json
import os
from pathlib import Path
config=json.loads(Path(os.environ['COLUMN_BOUNDARY_CONFIG']).read_text())
state={'status':'FAIL','entries':0,'returns':0,'errors':[],'armed':False}
phases=config.get('phase_symbols')
outer=None
owner=None
def same_thread():
	return gdb.selected_thread().ptid==owner
def save():

	Path(config['gdb_receipt']).write_text(json.dumps(state,indent=2)+'\n')
class Return(gdb.FinishBreakpoint):
	def stop(self):
		try:
			if phases and (state.get('split') is None or not same_thread()):
				raise RuntimeError('return without an in-frame phase split')
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
		super().__init__("*'"+symbol+"'",internal=True)
		self.expected=name
	def stop(self):
		try:
			frame=gdb.newest_frame()
			if (frame.name()!=self.expected or state['entries']!=1 or state['returns'] or
					'split' in state or not same_thread()):
				raise RuntimeError('wrong or repeated invocation phase split')
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
			global outer,owner
			if phases:
				if state['entries']:
					raise RuntimeError('nested invocation')
				outer=gdb.newest_frame();owner=gdb.selected_thread().ptid
				state['thread']=owner
			else:
				self.enabled=False
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
entry=Entry("*'"+config['entry']+"'",internal=True)
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
