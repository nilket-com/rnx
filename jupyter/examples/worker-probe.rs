//! Standalone acknowledgement/watchdog gate. Never installed as a kernel.
#[cfg(target_os = "linux")]
#[tokio::main(flavor = "multi_thread", worker_threads = 2)]
async fn main() -> rnx_jupyter::transport::Res<()> {
	use rnx_jupyter::{
		containment,
		worker::{Interrupt, StreamEvent, Worker},
	};
	use std::{path::PathBuf, sync::Arc};
	use tokio::time::{Duration, Instant};
	containment::initialize()?;
	let args = std::env::args().collect::<Vec<_>>();
	let binary = PathBuf::from(&args[1]);
	let phase = &args[2];
	let mut worker = Worker::spawn(&binary, Arc::new(Interrupt::default())).await?;
	let start = Instant::now();
	let mut sink = |_: StreamEvent| async {
		if phase == "stream" {
			std::future::pending::<()>().await;
		}
		Ok(())
	};
	let mut complete = |_: serde_json::Value| async {
		if phase == "result" {
			std::future::pending::<()>().await;
		}
		Ok(())
	};
	let result = worker.operate(Some("42"), &mut sink, &mut complete).await;
	let elapsed = start.elapsed().as_secs_f64();
	worker
		.terminate(Instant::now() + Duration::from_secs(5))
		.await?;
	assert!(result.is_err());
	assert!(
		result
			.as_ref()
			.err()
			.unwrap()
			.to_string()
			.contains("settlement/publication deadline")
	);
	assert!((4.9..6.0).contains(&elapsed));
	println!(
		"{}",
		serde_json::json!({"case":"blocked_handoff_no_ack","phase":phase,"seconds":elapsed})
	);
	Ok(())
}
#[cfg(not(target_os = "linux"))]
fn main() {
	panic!("worker probe needs Linux");
}
