"""Check the reviewed same-directory production pair, never a cross-host hash."""
import hashlib
from pathlib import Path
import platform
import struct


def validate_pair_metadata(pair, baseline):
	assert pair['status'] == 'PASS'
	rows = pair['builds']
	assert len(rows) == 2 and [row['role'] for row in rows] == ['baseline', 'candidate']
	assert rows[0]['source_commit'] == baseline
	assert rows[0]['command'] == rows[1]['command']
	assert rows[0]['command'] == ['cargo', '+1.98.1', 'build', '--locked', '--release', '--manifest-path', 'adapters/polars/Cargo.toml', '--bin', 'rnx-polars']
	for name in ('.text', '.rodata'):
		assert rows[0]['sections'][name]['size'] > 0
		assert rows[0]['sections'][name] == rows[1]['sections'][name], ('production section differs', name)
	return rows


def verify_pair(pair, baseline):
	rows = validate_pair_metadata(pair, baseline)
	paths = [Path(row['binary']).resolve(strict=True) for row in rows]
	assert paths[0].parent == paths[1].parent and paths[0] != paths[1], 'expected saved outputs of one paired build'
	for row, path in zip(rows, paths):
		data = path.read_bytes()
		assert hashlib.sha256(data).hexdigest() == row['sha256'], 'paired binary changed'
		assert data[:6] == b'\x7fELF\x02\x01'
		offset = struct.unpack_from('<Q', data, 40)[0]
		size, count, strings = struct.unpack_from('<HHH', data, 58)
		headers = [struct.unpack_from('<IIQQQQIIQQ', data, offset + i * size) for i in range(count)]
		h = headers[strings]
		names = data[h[4]:h[4]+h[5]]
		found = {}
		for h in headers:
			name = names[h[0]:].split(b'\0', 1)[0].decode()
			if name in ('.text', '.rodata'):
				found[name] = {'size': h[5], 'sha256': hashlib.sha256(data[h[4]:h[4]+h[5]]).hexdigest()}
		assert found == {name: row['sections'][name] for name in ('.text', '.rodata')}
	return {'status': 'PASS', 'criterion': 'same-directory paired .text and .rodata identity',
		'verification_host': platform.node(), 'paired_binary_directory': str(paths[0].parent),
		'baseline': rows[0]['source_commit'], 'candidate': rows[1]['source_commit'],
		'whole_binary_identical': rows[0]['sha256'] == rows[1]['sha256'],
		'sections': {name: rows[0]['sections'][name] for name in ('.text', '.rodata')}}
