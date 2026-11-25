//! The final's own high water marks, by filesystem and aggregate cgroup shmem.
use crate::{controller::disk_free, receipt::atomic_json};
use anyhow::{Context, Result};
use serde_json::{Value, json};
use std::{
	fs,
	path::{Path, PathBuf},
	sync::mpsc,
	thread,
	time::Duration,
};

pub struct Sampler {
	stop: mpsc::Sender<()>,
	thread: thread::JoinHandle<Result<()>>,
}

impl Sampler {
	pub fn start(
		run: PathBuf,
		out: PathBuf,
		stop_stages: impl Fn() + Send + 'static,
	) -> Result<Self> {
		let membership = fs::read_to_string("/proc/self/cgroup")?;
		let unit = membership
			.lines()
			.find_map(|s| s.strip_prefix("0::/"))
			.context("no unified cgroup")?;
		let cgroup = Path::new("/sys/fs/cgroup").join(unit);
		let state = run.join("space.json");
		let mut rows: Value = serde_json::from_slice(&fs::read(&state)?)?;
		let mut shortage = None;
		let (stop, receiver) = mpsc::channel();
		let thread = thread::spawn(move || {
			loop {
				for (name, path) in [("build", &run), ("receipts", &out)] {
					let Some(row) = rows.get_mut(name) else {
						continue;
					};
					let free = disk_free(path)?;
					if free < crate::controller::DISK_FLOOR_BYTES && shortage.is_none() {
						shortage = Some(format!(
							"low free space at {}: {free} bytes",
							path.display()
						));
						stop_stages();
					}
					let peak = row["start"]
						.as_u64()
						.context("missing start space")?
						.saturating_sub(free);
					row["peak"] = json!(peak.max(row["peak"].as_u64().context("missing peak")?));
				}
				let stat = fs::read_to_string(cgroup.join("memory.stat"))?;
				let used: u64 = stat
					.lines()
					.find_map(|l| l.strip_prefix("shmem "))
					.context("missing shmem")?
					.parse()?;
				let mem = &mut rows["memory"];
				mem["peak"] = json!(
					used.saturating_sub(mem["start"].as_u64().context("missing shmem start")?)
						.max(mem["peak"].as_u64().context("missing shmem peak")?)
				);
				if !matches!(
					receiver.recv_timeout(Duration::from_millis(250)),
					Err(mpsc::RecvTimeoutError::Timeout)
				) {
					break;
				}
			}
			atomic_json(&state, &rows)?;
			if let Some(detail) = shortage {
				anyhow::bail!("{detail}")
			}
			Ok(())
		});
		Ok(Self { stop, thread })
	}

	pub fn finish(self) -> Result<()> {
		let _ = self.stop.send(());
		self.thread
			.join()
			.map_err(|_| anyhow::anyhow!("filesystem sampler panicked"))?
	}
}
