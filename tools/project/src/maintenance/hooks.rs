use crate::commands;
pub(super) fn point(name: &str) -> Result<(), String> {
	commands::check()?;
	#[cfg(feature = "test-support")]
	{
		if std::env::var("RNX_REMOVE_PAUSE").as_deref() == Ok(name) {
			use std::io::Write;
			println!("{}", serde_json::json!({"paused":name}));
			std::io::stdout().flush().map_err(|e| e.to_string())?;
			let release =
				std::env::var_os("RNX_REMOVE_RELEASE").ok_or("pause needs release path")?;
			while !std::path::Path::new(&release).exists() {
				commands::check()?;
				std::thread::sleep(std::time::Duration::from_millis(10));
			}
		}
		if std::env::var("RNX_REMOVE_FAIL").as_deref() == Ok(name) {
			return Err(format!("injected removal failure at {name}"));
		}
	}
	let _ = name;
	commands::check()
}
pub(super) fn limit(name: &str, normal: u64) -> u64 {
	#[cfg(feature = "test-support")]
	if let Ok(v) = std::env::var(name) {
		return v.parse::<u64>().unwrap_or(0).min(normal);
	}
	let _ = name;
	normal
}
pub(super) fn open_error() -> Option<i32> {
	#[cfg(feature = "test-support")]
	{
		std::env::var("RNX_REMOVE_OPEN_ERRNO")
			.ok()
			.and_then(|v| v.parse().ok())
	}
	#[cfg(not(feature = "test-support"))]
	{
		None
	}
}
