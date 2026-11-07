import itertools
import unittest
from huge_page_faults import evaluate


class Controls(unittest.TestCase):
	def cases(self):
		return [dict(kind=k, layout=l, call=i, baseline_minor=1000, candidate_minor=100)
			for k,l,i in itertools.product(('f64const','f64add','f64chain'),('flat','alternating'),range(3))]

	def test_boundary_and_cold_outlier(self):
		rows=self.cases(); self.assertEqual(evaluate(rows)['status'],'PASS')
		rows[0]['candidate_minor']=999; self.assertEqual(evaluate(rows)['status'],'PASS')
		rows[0]['candidate_minor']=1001
		self.assertEqual(evaluate(rows)['failures'][0]['rule'],'faults exceed baseline')

	def test_median_and_coverage(self):
		rows=self.cases()
		for row in rows: row['candidate_minor']=101
		self.assertEqual(evaluate(rows)['failures'][0]['rule'],'median reduction below 10x')
		with self.assertRaises(ValueError): evaluate(rows[:-1])
		rows[-1]=dict(rows[0])
		with self.assertRaises(ValueError): evaluate(rows)


if __name__=='__main__': unittest.main()
