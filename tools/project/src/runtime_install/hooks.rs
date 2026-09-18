use crate::commands;
pub(super) fn point(name: &str) -> Result<(), String> {
	commands::check()?;
	#[cfg(feature = "test-support")]
	{
		if std::env::var("RNX_INSTALL_PAUSE").as_deref() == Ok(name) {
			let marker = std::path::PathBuf::from(
				std::env::var_os("RNX_INSTALL_MARKER").ok_or("pause marker missing")?,
			);
			std::fs::write(&marker, name).map_err(|e| e.to_string())?;
			while !marker.with_extension("release").exists() {
				commands::check()?;
				std::thread::sleep(std::time::Duration::from_millis(10));
			}
		}
		if std::env::var("RNX_INSTALL_FAIL").as_deref() == Ok(name) {
			return Err(format!("injected installer failure: {name}"));
		}
	}
	let _ = name;
	commands::check()
}
pub(super) fn limit(name: &str, ordinary: u64) -> u64 {
	#[cfg(feature = "test-support")]
	if let Ok(v) = std::env::var(name) {
		return v.parse::<u64>().unwrap_or(0).min(ordinary);
	}
	let _ = name;
	ordinary
}

pub(super) fn read(path: &std::path::Path, bytes: usize) -> Result<(), String> {
	#[cfg(feature = "test-support")]
	if let Some(log) = std::env::var_os("RNX_INSTALL_READ_LOG") {
		use std::io::Write;
		let mut f = std::fs::OpenOptions::new()
			.create(true)
			.append(true)
			.open(log)
			.map_err(|e| e.to_string())?;
		writeln!(f, "{}", serde_json::json!({"path":path,"bytes":bytes}))
			.map_err(|e| e.to_string())?;
	}
	let _ = (path, bytes);
	Ok(())
}
