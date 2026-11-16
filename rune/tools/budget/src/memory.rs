//! Parallelism from the detected CPUs and memory, never a fixed count.
//!
//! Memory is the smaller of the host's available memory and the usable
//! headroom under every finite `memory.high` and `memory.max` of this
//! process's control group and its ancestors, so a final inside a systemd
//! unit is sized by the unit's limits. Reclaimable page cache counts as
//! headroom; nothing is written to the kernel to reclaim it.
use anyhow::{bail, Context as _, Result};
use serde::Serialize;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

pub const GIB: u64 = 1024 * 1024 * 1024;
/// Left free for the controller, the shell and Cargo's own resolver.
pub const RESERVE_BYTES: u64 = 2 * GIB;
/// A rustc job on the Polars crates peaks near this.
pub const JOB_MEMORY_BYTES: u64 = 3 * GIB;
/// A native contract case: one test thread and its interpreter children.
pub const CASE_MEMORY_BYTES: u64 = 2 * GIB;
/// A final's stages share one control group with their page cache and test
/// children; a group pushed to its high limit is throttled until
/// deadline-bounded tests fail, so a quarter of its memory stays free.
pub const FINAL_RESERVE_SHARE: f64 = 0.25;

#[derive(Clone, Debug, Serialize, PartialEq)]
pub struct Limit {
	pub limit_bytes: u64,
	pub raw_headroom_bytes: u64,
	pub usable_bytes: u64,
}

#[derive(Clone, Debug, Serialize, PartialEq)]
pub struct Group {
	pub group: String,
	pub current_bytes: u64,
	pub fields: BTreeMap<String, u64>,
	pub cache_credit_bytes: u64,
	pub limits: BTreeMap<String, Option<Limit>>,
}

#[derive(Clone, Debug, Serialize, PartialEq)]
pub struct Observation {
	pub cpu_ids: Vec<usize>,
	pub mem_available_bytes: u64,
	pub ancestor_headroom_bytes: u64,
	pub binding_cgroup: String,
	pub binding_limit: String,
	pub groups: Vec<Group>,
}

/// Where an observation reads from; tests point it at fixture trees.
pub struct Sources {
	pub cgroup_root: PathBuf,
	pub meminfo: PathBuf,
	pub membership: PathBuf,
	pub cpus: Option<Vec<usize>>,
}

impl Default for Sources {
	fn default() -> Self {
		Self {
			cgroup_root: "/sys/fs/cgroup".into(),
			meminfo: "/proc/meminfo".into(),
			membership: "/proc/self/cgroup".into(),
			cpus: None,
		}
	}
}

fn number(text: &str) -> Result<u64> {
	if text.is_empty() || !text.bytes().all(|b| b.is_ascii_digit()) {
		bail!("nonnegative decimal accounting value required");
	}
	Ok(text.parse()?)
}

fn affinity() -> Result<Vec<usize>> {
	// SAFETY: a zeroed cpu_set_t is a valid empty set for sched_getaffinity.
	let mut set: libc::cpu_set_t = unsafe { std::mem::zeroed() };
	if unsafe { libc::sched_getaffinity(0, std::mem::size_of::<libc::cpu_set_t>(), &mut set) } != 0
	{
		return Err(std::io::Error::last_os_error()).context("sched_getaffinity");
	}
	Ok((0..libc::CPU_SETSIZE as usize)
		.filter(|&cpu| unsafe { libc::CPU_ISSET(cpu, &set) })
		.collect())
}

/// Every finite high or max limit binds, up to the root.
pub fn observe(sources: &Sources) -> Result<Observation> {
	let cpu_ids = match &sources.cpus {
		Some(cpus) => cpus.clone(),
		None => affinity()?,
	};
	if cpu_ids.is_empty() {
		bail!("empty CPU affinity");
	}
	let meminfo =
		std::fs::read_to_string(&sources.meminfo).context("cannot read admission accounting")?;
	let available = meminfo
		.lines()
		.find_map(|line| line.strip_prefix("MemAvailable:"))
		.context("cannot read admission accounting: MemAvailable")?;
	let mut parts = available.split_whitespace();
	let (amount, unit) = (parts.next().unwrap_or(""), parts.next().unwrap_or(""));
	if unit != "kB" {
		bail!("unknown MemAvailable unit");
	}
	let available = number(amount)? * 1024;
	let membership =
		std::fs::read_to_string(&sources.membership).context("cannot read admission accounting")?;
	let unified: Vec<&str> = membership
		.lines()
		.filter_map(|line| line.strip_prefix("0::"))
		.collect();
	if unified.len() != 1 {
		bail!("unified membership missing or ambiguous");
	}
	let name = unified[0];
	if !name.starts_with('/') || name.split('/').any(|part| part == "..") {
		bail!("invalid control-group path");
	}
	let mut result = Observation {
		cpu_ids,
		mem_available_bytes: available,
		ancestor_headroom_bytes: available,
		binding_cgroup: "host".into(),
		binding_limit: "MemAvailable".into(),
		groups: Vec::new(),
	};
	let root = &sources.cgroup_root;
	let mut group = root.join(name.trim_start_matches('/'));
	while group != *root && group.starts_with(root) {
		let row = observe_group(root, &group)?;
		for key in ["memory.max", "memory.high"] {
			if let Some(Some(limit)) = row.limits.get(key) {
				if limit.usable_bytes < result.ancestor_headroom_bytes {
					result.ancestor_headroom_bytes = limit.usable_bytes;
					result.binding_cgroup = row.group.clone();
					result.binding_limit = key.into();
				}
			}
		}
		result.groups.push(row);
		group = match group.parent() {
			Some(parent) => parent.to_owned(),
			None => break,
		};
	}
	Ok(result)
}

fn observe_group(root: &Path, group: &Path) -> Result<Group> {
	let read = |name: &str| {
		std::fs::read_to_string(group.join(name)).with_context(|| {
			format!(
				"cannot read admission accounting: {}",
				group.join(name).display()
			)
		})
	};
	if std::fs::canonicalize(group)? != group {
		bail!("linked control group");
	}
	let current = number(read("memory.current")?.trim())?;
	let mut stats = BTreeMap::new();
	for line in read("memory.stat")?.lines() {
		let mut parts = line.split_whitespace();
		let (key, value) = (parts.next().unwrap_or(""), parts.next().unwrap_or(""));
		if stats.insert(key.to_owned(), number(value)?).is_some() {
			bail!("duplicate memory.stat key");
		}
	}
	let mut fields = BTreeMap::new();
	for key in [
		"file",
		"shmem",
		"inactive_file",
		"file_dirty",
		"file_writeback",
		"file_mapped",
	] {
		fields.insert(
			key.to_owned(),
			*stats
				.get(key)
				.with_context(|| format!("cannot read admission accounting: {key}"))?,
		);
	}
	let field = |key: &str| fields[key] as i128;
	let credit = (field("inactive_file").min((field("file") - field("shmem")).max(0))
		- field("file_dirty")
		- field("file_writeback")
		- field("file_mapped"))
	.max(0);
	let credit = (current as i128).min(credit) as u64;
	let mut limits = BTreeMap::new();
	for key in ["memory.max", "memory.high"] {
		let text = read(key)?;
		let text = text.trim();
		if text == "max" {
			limits.insert(key.to_owned(), None);
			continue;
		}
		let limit = number(text)?;
		limits.insert(
			key.to_owned(),
			Some(Limit {
				limit_bytes: limit,
				raw_headroom_bytes: limit.saturating_sub(current),
				usable_bytes: (limit as i128 - current as i128 + credit as i128).max(0) as u64,
			}),
		);
	}
	Ok(Group {
		group: format!("/{}", group.strip_prefix(root)?.display()),
		current_bytes: current,
		fields,
		cache_credit_bytes: credit,
		limits,
	})
}

/// Workers that fit: no more than the CPUs, each with `per_worker` bytes
/// after `reserve`. Fewer than one is an error naming the binding limit.
pub fn capacity(
	cpus: usize,
	observation: &Observation,
	reserve: u64,
	per_worker: u64,
) -> Result<usize> {
	if cpus == 0 || per_worker == 0 {
		bail!("invalid resource observation or reservation");
	}
	let ceiling = observation
		.mem_available_bytes
		.min(observation.ancestor_headroom_bytes)
		.saturating_sub(reserve);
	let workers = (cpus as u64).min(ceiling / per_worker) as usize;
	if workers < 1 {
		bail!(
			"InsufficientMemory: host_available_bytes={}, cgroup_headroom_bytes={}, binding_cgroup={}, binding_limit={}, reserve_bytes={reserve}, per_worker_bytes={per_worker}, workers=0",
			observation.mem_available_bytes,
			observation.ancestor_headroom_bytes,
			observation.binding_cgroup,
			observation.binding_limit
		);
	}
	Ok(workers)
}

/// Cargo jobs for one build.
pub fn build_jobs(observation: &Observation) -> Result<usize> {
	capacity(
		observation.cpu_ids.len(),
		observation,
		RESERVE_BYTES,
		JOB_MEMORY_BYTES,
	)
}

/// The whole final's tokens: a build's jobs with a quarter of its memory held back.
pub fn final_jobs(observation: &Observation) -> Result<usize> {
	let limit = observation
		.mem_available_bytes
		.min(observation.ancestor_headroom_bytes);
	let reserve = RESERVE_BYTES.max((limit as f64 * FINAL_RESERVE_SHARE) as u64);
	capacity(
		observation.cpu_ids.len(),
		observation,
		reserve,
		JOB_MEMORY_BYTES,
	)
}

/// Concurrent single-threaded cases: half the CPUs leave room for their children.
pub fn case_width(observation: &Observation) -> Result<usize> {
	capacity(
		(observation.cpu_ids.len() / 2).max(1),
		observation,
		RESERVE_BYTES,
		CASE_MEMORY_BYTES,
	)
}

/// This process's control group `memory.events` counters, if readable.
pub fn memory_events() -> Option<BTreeMap<String, u64>> {
	let membership = std::fs::read_to_string("/proc/self/cgroup").ok()?;
	let name = membership
		.lines()
		.find_map(|line| line.strip_prefix("0::"))?;
	let text = std::fs::read_to_string(format!("/sys/fs/cgroup{name}/memory.events")).ok()?;
	text.lines()
		.map(|row| {
			let mut parts = row.split_whitespace();
			Some((parts.next()?.to_owned(), parts.next()?.parse().ok()?))
		})
		.collect()
}

#[cfg(test)]
mod tests {
	use super::*;

	fn observation(cpus: usize, available: u64, headroom: u64) -> Observation {
		Observation {
			cpu_ids: (0..cpus).collect(),
			mem_available_bytes: available,
			ancestor_headroom_bytes: headroom,
			binding_cgroup: "fixture".into(),
			binding_limit: "fixture".into(),
			groups: Vec::new(),
		}
	}

	#[test]
	fn a_final_holds_a_quarter_of_its_memory_back() {
		let o = observation(28, 100 * GIB, 48 * GIB);
		assert_eq!(build_jobs(&o).unwrap(), 15);
		assert_eq!(final_jobs(&o).unwrap(), 12);
	}

	#[test]
	fn too_little_memory_names_the_binding_limit() {
		let o = observation(4, GIB, GIB);
		let error = build_jobs(&o).unwrap_err().to_string();
		assert!(
			error.starts_with("InsufficientMemory:") && error.contains("binding_cgroup=fixture"),
			"{error}"
		);
	}

	#[test]
	fn an_idle_host_is_bounded_by_its_cpus() {
		assert_eq!(
			build_jobs(&observation(4, 1024 * GIB, 1024 * GIB)).unwrap(),
			4
		);
	}

	#[test]
	fn cases_use_half_the_cpus() {
		assert_eq!(
			case_width(&observation(28, 100 * GIB, 100 * GIB)).unwrap(),
			14
		);
		assert_eq!(
			case_width(&observation(1, 100 * GIB, 100 * GIB)).unwrap(),
			1
		);
	}

	#[test]
	fn every_ancestor_limit_binds_and_cache_counts_as_headroom() {
		let base = std::env::temp_dir().join(format!("job-budget-memory-{}", std::process::id()));
		let _ = std::fs::remove_dir_all(&base);
		let root = base.join("cgroup");
		let leaf = root.join("user.slice/unit.service");
		std::fs::create_dir_all(&leaf).unwrap();
		let stat =
			"file 0\nshmem 0\ninactive_file 0\nfile_dirty 0\nfile_writeback 0\nfile_mapped 0\n";
		for (group, current, high, max, stat) in [
			(&leaf, 10 * GIB, "max".to_owned(), (54 * GIB).to_string(), stat.to_owned()),
			(&root.join("user.slice"), 20 * GIB, (40 * GIB).to_string(), "max".to_owned(), format!("file {}\nshmem 0\ninactive_file {}\nfile_dirty 0\nfile_writeback 0\nfile_mapped 0\n", 8 * GIB, 6 * GIB)),
		] {
			std::fs::write(group.join("memory.current"), current.to_string()).unwrap();
			std::fs::write(group.join("memory.high"), high).unwrap();
			std::fs::write(group.join("memory.max"), max).unwrap();
			std::fs::write(group.join("memory.stat"), stat).unwrap();
		}
		std::fs::write(
			base.join("meminfo"),
			format!("MemTotal: 1 kB\nMemAvailable: {} kB\n", 100 * GIB / 1024),
		)
		.unwrap();
		std::fs::write(base.join("membership"), "0::/user.slice/unit.service\n").unwrap();
		let o = observe(&Sources {
			cgroup_root: root.clone(),
			meminfo: base.join("meminfo"),
			membership: base.join("membership"),
			cpus: Some((0..28).collect()),
		})
		.unwrap();
		// The slice's high limit binds: 40 - 20 + 6 GiB of reclaimable cache.
		assert_eq!(o.ancestor_headroom_bytes, 26 * GIB);
		assert_eq!(
			(o.binding_cgroup.as_str(), o.binding_limit.as_str()),
			("/user.slice", "memory.high")
		);
		assert_eq!(o.groups.len(), 2);
		std::fs::remove_dir_all(&base).unwrap();
	}
}
