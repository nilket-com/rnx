import unittest
from unittest.mock import patch
from f64_output_edges import check

class Controls(unittest.TestCase):
	def test_presence_and_repeated_staging(self):
		fill=dict(caller='filled::<f64>',callee='memset',calls=1)
		stage=dict(caller='f64_plan::run::<T>',callee='memset',calls=2)
		for role,edges,passed in [('baseline',[fill],True),('baseline',[],False),('candidate',[],True),('candidate',[fill],False),('candidate',[stage],False)]:
			with patch('f64_output_edges.edges',return_value=edges):
				r=[dict(kind='f64chain',role=role,rows=4096,layout='flat',costs=[dict(profile='x')])]
				self.assertEqual(not check(r),passed)
