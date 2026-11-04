import gdb
import json
import os
from pathlib import Path
config=json.loads(Path(os.environ['COLUMN_BOUNDARY_CONFIG']).read_text())
state={'status':'FAIL','entries':0,'returns':0,'errors':[],'armed':False}
def save():
	Path(config['gdb_receipt']).write_text(json.dumps(state,indent=2)+'\n')
class Return(gdb.FinishBreakpoint):
	def stop(self):
		try:
			gdb.execute('monitor dump old-path')
			gdb.execute('monitor instrumentation off')
			state['returns']+=1
			save()
			return False
		except BaseException as e:
			state['errors'].append(repr(e));save();return True
	def out_of_scope(self):
		state['errors'].append('execution returned outside frame');save()
class Entry(gdb.Breakpoint):
	def stop(self):
		try:
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
