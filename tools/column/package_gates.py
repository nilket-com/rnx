"""Declared package quality recipes; engine-family lint coverage stays explicit."""
from pathlib import Path

# Existing target/toolchain recipes, shared by package checks and their lint twins.
PACKAGE_CHECKS = {
	'rune/no-std/Cargo.toml': ('package-no-std', [['cargo', '+nightly-2026-09-01', 'run', '--locked', '--manifest-path', 'rune/no-std/Cargo.toml', '--example', 'minimal']]),
	'rune/examples/Cargo.toml': ('package-examples', [['cargo', '+1.98.1', 'check', '--locked', '--manifest-path', 'rune/examples/Cargo.toml', '--all-targets']]),
	'rune/benches/Cargo.toml': ('package-benches', [['cargo', '+1.98.1', 'check', '--locked', '--manifest-path', 'rune/benches/Cargo.toml', '--benches']]),
	'rune/hegel_tests/Cargo.toml': ('package-hegel', [['cargo', '+1.98.1', 'test', '--manifest-path', 'rune/hegel_tests/Cargo.toml']]),
	'tools/polars-gen/Cargo.toml': ('package-polars-gen', [['cargo', '+1.98.1', 'check', '--locked', '--manifest-path', 'tools/polars-gen/Cargo.toml', '--all-targets']]),
}
DEMO_CHECKS = {
	m: ('check-' + m.replace('/', '-'), [['cargo', '+1.98.1', 'check', '--locked', '--all-targets', '--manifest-path', m]])
	for m in ('demos/candle/Cargo.toml', 'demos/polars/Cargo.toml', 'servers/http-postgres/Cargo.toml')
}
FORMATS = {
	'project-format': ['cargo', '+1.98.1', 'fmt', '--manifest-path', 'tools/project/Cargo.toml', '--all', '--check'],
	**{'format-adapter-' + n: ['cargo', '+1.98.1', 'fmt', '--manifest-path', f'adapters/{n}/Cargo.toml', '--all', '--check'] for n in ('candle', 'postgres')},
}
LINTS = {}
# manifest -> (test/check stages, format stage, lint stage)
PACKAGES = {
	'Cargo.toml': (('root-tests', 'inventory', 'cross'), 'format', 'clippy'),
	'tools/project/Cargo.toml': (('project-suite', 'cross'), 'project-format', 'project-clippy'),
	'jupyter/Cargo.toml': (('jupyter-tests', 'jupyter-kernel'), 'jupyter-format', 'jupyter-clippy'),
	**{f'adapters/{n}/Cargo.toml': ((f'suite-adapter-{n}',), 'adapter-format' if n == 'polars' else f'format-adapter-{n}', f'clippy-adapter-{n}') for n in ('candle', 'polars', 'postgres')},
}
for _manifest, (_stage, _commands) in (PACKAGE_CHECKS | DEMO_CHECKS).items():
	_format, _lint = 'format-' + _stage, 'lint-' + _stage
	FORMATS[_format] = ['cargo', '+1.98.1', 'fmt', '--manifest-path', _manifest, '--all', '--check']
	_lints = []
	for _command in _commands:
		_l = list(_command)
		_l[2] = 'clippy'
		if _command[2] == 'test': _l.append('--tests')
		_lints.append(_l)
	LINTS[_lint] = _lints
	PACKAGES[_manifest] = ((_stage,), _format, _lint)


def quality(manifest):
	if manifest.startswith('rune/crates/'):
		# Deliberately the existing Rune-family feature matrix, not primary-package
		# lint of every upstream workspace member.
		return (('rune-suite', 'cross'), 'format', 'rune-clippy')
	if manifest not in PACKAGES:
		raise ValueError(f'package {manifest}: declare its test/check, format and lint recipes in package_gates.py')
	return PACKAGES[manifest]


def source_owner(path, manifests):
	owners = [m for m in manifests if Path(path).is_relative_to(Path(m).parent)]
	if not owners: return None
	manifest = max(owners, key=lambda m: len(Path(m).parts))
	relative = Path(path).relative_to(Path(manifest).parent)
	if relative.name == 'build.rs' or relative.parts[0] in {'src', 'tests', 'examples', 'benches'} or (len(relative.parts) == 1 and relative.suffix == '.rs'):
		return manifest
	return None
