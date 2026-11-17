"""Package check, format and lint recipes, as suite_stage runs them.

Which of them a final requires is the Rust controller's rule (tools/final)."""

# Existing target/toolchain recipes, shared by package checks and their lint twins.
PACKAGE_CHECKS = {
	'rune/no-std/Cargo.toml': ('package-no-std', [['cargo', '+nightly-2026-09-01', 'run', '--locked', '--manifest-path', 'rune/no-std/Cargo.toml', '--example', 'minimal']]),
	'rune/examples/Cargo.toml': ('package-examples', [['cargo', '+1.98.1', 'check', '--locked', '--manifest-path', 'rune/examples/Cargo.toml', '--all-targets']]),
	'rune/benches/Cargo.toml': ('package-benches', [['cargo', '+1.98.1', 'check', '--locked', '--manifest-path', 'rune/benches/Cargo.toml', '--benches']]),
	'rune/hegel_tests/Cargo.toml': ('package-hegel', [['cargo', '+1.98.1', 'test', '--manifest-path', 'rune/hegel_tests/Cargo.toml']]),
	'rune/tools/xtask/column/probe/Cargo.toml': ('package-column-probe', [['cargo', '+1.98.1', 'check', '--locked', '--all-targets', '--all-features', '--manifest-path', 'rune/tools/xtask/column/probe/Cargo.toml']]),
	'rune/tools/xtask/measure/harness/Cargo.toml': ('package-measure-harness', [['cargo', '+1.98.1', 'check', '--locked', '--all-targets', '--all-features', '--manifest-path', 'rune/tools/xtask/measure/harness/Cargo.toml']]),
	'rune/tools/xtask/Cargo.toml': ('package-xtask', [['cargo', '+1.98.1', 'test', '--locked', '--all-targets', '--all-features', '--manifest-path', 'rune/tools/xtask/Cargo.toml']]),
	'tools/polars-gen/Cargo.toml': ('package-polars-gen', [['cargo', '+1.98.1', 'check', '--locked', '--manifest-path', 'tools/polars-gen/Cargo.toml', '--all-targets']]),
	**{m: ('package-' + m.split('/')[-2].replace('_', '-'), [['cargo', '+1.98.1', 'check', '--locked', '--manifest-path', m, '--all-targets']])
		for m in ('ci/execute_guard/Cargo.toml', 'rune/tools/builder/Cargo.toml', 'rune/tools/generate/Cargo.toml', 'rune/tools/site/Cargo.toml')},
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

