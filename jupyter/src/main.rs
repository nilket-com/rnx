use std::path::PathBuf;
fn main() {
	if let Err(e) = entry() {
		eprintln!("rnx-jupyter: {e:?}");
		std::process::exit(1);
	}
}
fn entry() -> rnx_jupyter::transport::Res<()> {
	let mut args = std::env::args_os().skip(1).peekable();
	let install = args.peek().is_some_and(|s| s == "install");
	if install {
		args.next();
	}
	let mut replace = false;
	let mut connection = None;
	let mut worker = None;
	while let Some(arg) = args.next() {
		match arg.to_str() {
			Some("--replace") if install && !replace => {
				replace = true;
			}
			Some("--connection-file") if !install && connection.is_none() => {
				connection = Some(PathBuf::from(
					args.next().ok_or("--connection-file needs a path")?,
				))
			}
			Some("--rnx") if worker.is_none() => {
				worker = Some(PathBuf::from(
					args.next()
						.ok_or("--rnx needs an absolute executable path")?,
				))
			}
			Some("--help") => {
				println!(
					"rnx-jupyter --connection-file FILE --rnx ABSOLUTE_EXECUTABLE\nrnx-jupyter install --rnx ABSOLUTE_EXECUTABLE [--replace]"
				);
				return Ok(());
			}
			_ => {
				return Err(
					"usage: rnx-jupyter --connection-file FILE --rnx ABSOLUTE_EXECUTABLE".into(),
				);
			}
		}
	}
	if install {
		return rnx_jupyter::install::install(&worker.ok_or("--rnx is required")?, replace);
	}
	let connection = rnx_jupyter::connection::Connection::read(
		&connection.ok_or("--connection-file is required")?,
	)?;
	let worker = worker.ok_or("--rnx is required")?;
	#[cfg(target_os = "linux")]
	{
		rnx_jupyter::containment::initialize()?;
		let runtime = tokio::runtime::Builder::new_multi_thread()
			.worker_threads(2)
			.enable_all()
			.build()?;
		runtime.block_on(rnx_jupyter::kernel::run(connection, &worker))
	}
	#[cfg(not(target_os = "linux"))]
	{
		let _ = (connection, worker);
		Err("worker supervision is implemented on Linux only in this interim build".into())
	}
}
