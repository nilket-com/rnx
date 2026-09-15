//! Standard stream operations; distinct from Rune's ::std::io printing module.
use rune::{Context, Module};

pub(crate) fn install(context: &mut Context) -> crate::Result<Vec<crate::host::HostFunction>> {
	let mut module = Module::with_crate("io")?;
	module.function("stdin", crate::host::stdin_read).build()?;
	module.function("eprint", crate::host::eprint).build()?;
	context.install(module)?;
	Ok(vec![
		crate::host::HostFunction {
			path: "io::stdin".into(),
			doc: "stdin() -> Result<String>: the whole of standard input as UTF-8, up to 8 MiB; Err on a terminal or a second read; not notebook input_request",
		},
		crate::host::HostFunction {
			path: "io::eprint".into(),
			doc: "eprint(text) -> Result<()>: write text to standard error, adding nothing, then flush",
		},
	])
}
