import unittest
import f64_output_pins as pins


class Pins(unittest.TestCase):
	def test_every_pin_and_plus_one(self):
		for rule in pins.POLICY['rules']:
			for rows in (4096,65536,10000000,100000000):
				a,b={},{}
				for name,p in rule['functions'].items():
					delta=p['constant_Ir']+p['per_block_Ir']*((rows+1023)//1024)
					base=max(0,-delta)+100
					a[name]=(base,p['calls'][0]);b[name]=(base+delta,p['calls'][1])
				args=(rule['kind'],rule['layout'],rows)
				self.assertFalse(pins.check_vectors(a,b,*args)[0])
				for name in b:
					for field in (0,1):
						bad=dict(b);v=list(bad[name]);v[field]+=1;bad[name]=tuple(v)
						self.assertIn(name,pins.check_vectors(a,bad,*args)[0])

	def test_unlisted_and_pair(self):
		pins.verify_pair(pins.POLICY['builds'])
		with self.assertRaises(ValueError): pins.verify_pair([])
		bad=[dict(b) for b in pins.POLICY['builds']];bad[1]['sha256']='wrong'
		with self.assertRaises(ValueError): pins.verify_pair(bad)
		self.assertIn('new',pins.check_vectors({}, {'new':(1,1)}, 'booland','flat',4096)[0])
