"""Native admission declarations never alter checksums or performance limits."""
import copy
import hashlib
import json
from pathlib import Path
import sys
import unittest
sys.path.insert(0,str(Path(__file__).resolve().parent))
import counter_packet
import counter_schedule
import test_counter_packet


def digest(value):
	return hashlib.sha256(value.encode()).hexdigest()


def fixture_facts():
	commits={role:{'candidate':'a','tip':'b','base':'c'}[role.split('-')[0]]*40 for role in counter_schedule.SUBJECTS}
	bindings={role:dict(commit=commits[role],binary_sha256=digest(role),kernels={k:dict(source_sha256=digest(k),stream_sha256=digest(stream))for k,stream in counter_schedule.KERNELS})for role in commits}
	trace=dict(status='PASS',inferior_exit=dict(exit_code=0),events=[dict(status=1,returned=1,mapping='1000-3000 r-xp 00000000 00:00 0',target='0x1100')for _ in range(2)])
	data=json.dumps(trace,indent=2)+'\n'
	receipt=dict(status='PASS',candidate=commits['candidate-on'],binary=dict(sha256=digest('candidate-on')),source=dict(sha256=digest('fib')),stream=dict(sha256=digest('fib_main')),trace=dict(sha256=hashlib.sha256(data.encode()).hexdigest(),bytes=len(data.encode())),entries=2,completions=2,replays=0)
	facts=dict(schema=1,entries=[dict(role='candidate-on',kernel='fib',source_commit=commits['candidate-on'],binary_sha256=digest('candidate-on'),maps=1,kib=8,proof=dict(receipt=receipt,trace=data))])
	return facts,bindings,commits


class NativeMappingTests(unittest.TestCase):
	def packet(self):
		f=test_counter_packet.CounterPacketTests();f.setUp();self.addCleanup(f.doCleanups)
		facts,bindings,commits=fixture_facts()
		for role,binary in f.spec['binaries'].items():f.spec['pins'][binary]=bindings[role]['binary_sha256']
		for k,stream in counter_schedule.KERNELS:
			f.spec['pins'][str(Path(f.spec['logical_kernels'])/(k+'.rn'))]=digest(k)
			f.manifest['streams'][stream]['sha256']=digest(stream)
		payload=json.dumps(facts).encode();name='NATIVE-MAPPINGS.json';source='/logical/native-mappings.json';(f.root/name).write_bytes(payload)
		f.spec.update(native_mappings=dict(file=name,path=source,sha256=hashlib.sha256(payload).hexdigest(),bytes=len(payload)),role_commits=commits)
		f.spec['pins'][source]=hashlib.sha256(payload).hexdigest();f.records[0]['pins']=dict(f.spec['pins'])
		for row in f.records:
			if row.get('record')in('admission','row')and row['identity']['subject']=='candidate-on'and row['identity']['kernel']=='fib':
				prefix=Path(row['execution']['stdout']).name.removesuffix('.stdout');stdout=f"CHECKSUM 0 invocations={row['identity']['k']}\nMAPS anon_exec_mappings=1 anon_exec_kib=8\n".encode();f.raw(prefix,stdout);row['stdout_sha256']=hashlib.sha256(stdout).hexdigest()
		(f.root/'MANIFEST.json').write_text(json.dumps(f.manifest));(f.root/'session/records.jsonl').write_text(''.join(json.dumps(row)+'\n'for row in f.records))
		return f

	def test_exact_mapping_and_unchanged_default_cells(self):
		facts,bindings,_=fixture_facts();overrides=counter_packet.native_mapping_facts(facts,bindings)
		self.assertEqual(counter_packet.expected_native_mapping('candidate-on','fib',overrides),(1,8))
		for role in ('tip-on','candidate-off','base-on'):self.assertEqual(counter_packet.expected_native_mapping(role,'fib',overrides),(0,0))
		self.assertEqual(counter_packet.expected_native_mapping('candidate-on','shared_rows',overrides),(0,0))
		self.assertEqual(counter_packet.expected_native_mapping('candidate-on','while',overrides),(1,4))

	def test_wrong_role_source_binary_kernel_count_size_or_proof_rejected(self):
		for key,value in [('role','tip-on'),('role','candidate-off'),('source_commit','d'*40),('binary_sha256','d'*64),('kernel','while'),('kernel','unknown'),('maps',2),('kib',4)]:
			facts,bindings,_=fixture_facts();facts['entries'][0][key]=value
			with self.assertRaises(ValueError):counter_packet.native_mapping_facts(facts,bindings)
		facts,bindings,_=fixture_facts();facts['entries'][0]['proof']['trace']+=' '
		with self.assertRaises(ValueError):counter_packet.native_mapping_facts(facts,bindings)

	def test_mapping_packet_is_independently_reparsed(self):
		f=self.packet();result=f.run_package();self.assertEqual(result['workloads'],150)
		self.assertFalse(any(c['instruction_rule']['rejected']for c in result['cells']))

	def test_unlisted_native_mapping_and_changed_declaration_rejected(self):
		f=self.packet()
		row=next(r for r in f.records if r.get('record')=='admission'and r['identity']['subject']=='tip-on'and r['identity']['kernel']=='fib')
		prefix=Path(row['execution']['stdout']).name.removesuffix('.stdout');stdout=f"CHECKSUM 0 invocations={row['identity']['k']}\nMAPS anon_exec_mappings=1 anon_exec_kib=8\n".encode();f.raw(prefix,stdout);row['stdout_sha256']=hashlib.sha256(stdout).hexdigest();(f.root/'session/records.jsonl').write_text(''.join(json.dumps(r)+'\n'for r in f.records))
		with self.assertRaisesRegex(ValueError,'analyzer failed'):f.run_package()
		g=self.packet();p=g.root/'NATIVE-MAPPINGS.json';p.write_text(p.read_text()+' ')
		with self.assertRaisesRegex(ValueError,'analyzer failed'):g.run_package()


if __name__=='__main__':unittest.main()
