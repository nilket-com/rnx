//! Explicit installation, outside the serving process and its child ownership.
use crate::transport::Res;
use serde_json::json;
use std::{
	fs,
	path::{Path, PathBuf},
	process::Command,
};
struct Temporary(PathBuf);
impl Drop for Temporary {
	fn drop(&mut self) {
		let _ = fs::remove_dir_all(&self.0);
	}
}
fn text(path: &Path) -> Res<&str> {
	path.to_str()
		.ok_or_else(|| format!("cannot install non-Unicode path {:?}", path.as_os_str()).into())
}
fn jupyter(args: &[&str]) -> Res<std::process::Output> {
	let output = Command::new("jupyter").args(args).output().map_err(|e| {
		format!("cannot run jupyter: {e}; install the Jupyter CLI and put it on PATH")
	})?;
	if !output.status.success() {
		return Err(format!(
			"jupyter {} failed ({}): {}",
			args.join(" "),
			output.status,
			String::from_utf8_lossy(&output.stderr)
		)
		.into());
	}
	Ok(output)
}
pub fn install(worker: &Path, replace: bool) -> Res<()> {
	install_with_engine(worker, replace, crate::engine::Engine::Legacy)
}
pub fn install_with_engine(worker: &Path, replace: bool, engine: crate::engine::Engine) -> Res<()> {
	let name = engine.kernel_name();
	if !worker.is_absolute() {
		return Err("--rnx needs an absolute executable path".into());
	}
	let worker_text = text(worker)?;
	if !fs::metadata(worker)
		.map_err(|e| format!("cannot inspect worker {worker:?}: {e}"))?
		.is_file()
	{
		return Err(format!("worker {worker:?} is not a regular file").into());
	}
	let kernel = std::env::current_exe()?;
	let kernel_text = text(&kernel)?;
	// The pinned CLI ignores --replace and unconditionally removes a destination.
	// Inspect both discovery and the reported user destination, including invalid
	// specs and dangling links omitted by `kernelspec list`.
	let list = jupyter(&["kernelspec", "list", "--json"])?;
	let list: serde_json::Value = serde_json::from_slice(&list.stdout)?;
	let specs = list["kernelspecs"]
		.as_object()
		.ok_or("jupyter kernelspec list did not return a kernelspecs object")?;
	let data = jupyter(&["--data-dir"])?;
	let data = std::str::from_utf8(&data.stdout)?.trim_end_matches(['\r', '\n']);
	let data = Path::new(data);
	if !data.is_absolute() {
		return Err("jupyter --data-dir did not return an absolute path".into());
	}
	let target = data.join("kernels").join(name);
	let exists = match fs::symlink_metadata(&target) {
		Ok(_) => true,
		Err(e) if e.kind() == std::io::ErrorKind::NotFound => false,
		Err(e) => return Err(format!("cannot inspect {target:?}: {e}").into()),
	};
	if !replace && (exists || specs.keys().any(|k| k.eq_ignore_ascii_case(name))) {
		return Err(
			format!("an {name} kernelspec already exists; pass --replace to replace it").into(),
		);
	}
	let path = std::env::temp_dir().join(format!("rnx-kernelspec-{}", uuid::Uuid::new_v4()));
	let mut builder = fs::DirBuilder::new();
	#[cfg(unix)]
	{
		use std::os::unix::fs::DirBuilderExt;
		builder.mode(0o700);
	}
	builder.create(&path)?;
	let temporary = Temporary(path);
	let spec = specification(kernel_text, worker_text, engine);
	fs::write(
		temporary.0.join("kernel.json"),
		serde_json::to_vec_pretty(&spec)?,
	)?;
	let mut args = vec!["kernelspec", "install", "--user", "--name", name];
	if replace {
		args.push("--replace");
	}
	args.push(text(&temporary.0)?);
	jupyter(&args)?;
	println!(
		"Installed {}. Restart JupyterLab if it is already running.",
		engine.display_name()
	);
	Ok(())
}

fn specification(kernel: &str, worker: &str, engine: crate::engine::Engine) -> serde_json::Value {
	let mut argv = vec![
		kernel,
		"--connection-file",
		"{connection_file}",
		"--rnx",
		worker,
	];
	if engine == crate::engine::Engine::Execution {
		argv.push("--engine=execution");
	}
	json!({"argv":argv,"display_name":engine.display_name(),"language":"rune","interrupt_mode":"message"})
}
#[cfg(test)]
mod tests {
	#[test]
	fn default_spec_bytes_unchanged_and_opt_in_is_separate() {
		use crate::engine::Engine;
		let old = serde_json::json!({"argv":["/kernel","--connection-file","{connection_file}","--rnx","/worker"],"display_name":"Rune (rnx)","language":"rune","interrupt_mode":"message"});
		assert_eq!(
			serde_json::to_vec_pretty(&old).unwrap(),
			serde_json::to_vec_pretty(&super::specification("/kernel", "/worker", Engine::Legacy))
				.unwrap()
		);
		let new = super::specification("/kernel", "/worker", Engine::Execution);
		assert_eq!(new["argv"][5], "--engine=execution");
		assert_ne!(
			Engine::Legacy.kernel_name(),
			Engine::Execution.kernel_name()
		);
	}
}
