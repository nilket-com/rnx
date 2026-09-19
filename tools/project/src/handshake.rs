//! Bounded direct-child capability exchange; never invokes a script.
#![allow(dead_code)]
use std::{path::Path, process::Stdio, time::Duration};
use tokio::io::AsyncReadExt;
async fn read(stream: impl tokio::io::AsyncRead + Unpin) -> Result<Vec<u8>, String> {
	let mut bytes = vec![];
	stream
		.take(4097)
		.read_to_end(&mut bytes)
		.await
		.map_err(|e| e.to_string())?;
	if bytes.len() > 4096 {
		return Err("capability stream exceeds 4096 bytes".into());
	}
	Ok(bytes)
}

pub(crate) fn check(executable: &Path) -> Result<(), String> {
	let result = || -> Result<(), String> {
		let runtime = tokio::runtime::Builder::new_current_thread()
			.enable_all()
			.build()
			.map_err(|e| e.to_string())?;
		runtime.block_on(async {
			let mut child = tokio::process::Command::new(executable)
				.arg("project-source-version")
				.stdin(Stdio::null())
				.stdout(Stdio::piped())
				.stderr(Stdio::piped())
				.kill_on_drop(true)
				.spawn()
				.map_err(|e| e.to_string())?;
			let stdout = child.stdout.take().ok_or("missing handshake stdout")?;
			let stderr = child.stderr.take().ok_or("missing handshake stderr")?;

			let result = tokio::time::timeout(Duration::from_secs(1), async {
				let (stdout, stderr, status) =
					tokio::try_join!(read(stdout), read(stderr), async {
						child.wait().await.map_err(|e| e.to_string())
					})?;
				if !status.success() {
					return Err("capability command failed".into());
				}
				if !stderr.is_empty() {
					return Err("capability command wrote to stderr".into());
				}
				#[derive(serde::Deserialize)]
				#[serde(deny_unknown_fields)]
				struct Reply {
					format: u32,
				}
				let reply: Reply = serde_json::from_slice(&stdout)
					.map_err(|e| format!("invalid capability reply: {e}"))?;
				if reply.format != 1 {
					return Err("unsupported project-source version".into());
				}
				Ok::<(), String>(())
			})
			.await
			.unwrap_or_else(|_| Err("capability command exceeded one second".into()));
			// wait() above may already have reaped the direct child. On every
			// refusal, finish retirement before returning to the project command.
			if result.is_err() {
				let _ = child.kill().await;
				child.wait().await.map_err(|e| e.to_string())?;
			}
			result
		})
	};
	result().map_err(|e| format!("executable {}: {e}", executable.display()))
}

#[cfg(test)]
mod tests {
	#[test]
	fn capability_stream_limit_is_independent_of_process_scheduling() {
		let runtime = tokio::runtime::Builder::new_current_thread()
			.build()
			.unwrap();
		runtime.block_on(async {
			let boundary = vec![b' '; 4096];
			assert_eq!(super::read(boundary.as_slice()).await.unwrap(), boundary);
			let excess = vec![b'x'; 8192];
			let mut stream = excess.as_slice();
			assert_eq!(
				super::read(&mut stream).await.unwrap_err(),
				"capability stream exceeds 4096 bytes"
			);
			assert_eq!(stream.len(), 8192 - 4097);
		});
	}
}
