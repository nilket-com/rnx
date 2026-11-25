//! ADR 0335's frozen workload. The spans are observations of 0334 FINAL-v1.
//! Admission in the prediction uses durations; observed starts are used only
//! to validate the historical schedule and account for its timed overhead.
use rnx_final::{
	catalogue,
	scheduler::{self, Resources},
};
use std::collections::BTreeSet;

const SECOND: u64 = 1_000_000;
const TOTAL: u64 = 3_591_340_203;
const OUTSIDE_STAGES: u64 = 52_890_825;
const DEBUG: &str = "target/debug";
const SUITE: &str = "rune-suite";
// Sequential wall intervals between the stage context and each completed
// test log, rounded up to milliseconds. Nested test summaries overlap.
const RECIPE_MS: [u64; 12] = [
	90734, 134631, 54890, 182751, 38954, 1207, 26053, 4279, 3700, 3544, 3758, 3829,
];
// Completion of the contract binary build + cleanup in the child's receipt,
// relative to the last test log. The manifest then runs one exclusive case.
const CONTRACT_BUILD_MS: u64 = 53_625;
const EXCLUSIVE_WORK: u64 = 92_246_770;

struct Work {
	names: Vec<String>,
	starts: Vec<u64>,
	durations: Vec<u64>,
	weights: Vec<usize>,
	lanes: Vec<Resources>,
	parents: Vec<Vec<usize>>,
	suite: usize,
}

fn witness() -> Work {
	let mut work = Work {
		names: vec![],
		starts: vec![],
		durations: vec![],
		weights: vec![],
		lanes: vec![],
		parents: vec![],
		suite: 0,
	};
	for line in include_str!("witness/schedule-0334.tsv")
		.lines()
		.filter(|line| !line.starts_with('#'))
	{
		let [name, start, duration, weight, lanes]: [&str; 5] =
			line.split('\t').collect::<Vec<_>>().try_into().unwrap();
		work.names.push(name.into());
		work.starts.push(start.parse().unwrap());
		work.durations.push(duration.parse().unwrap());
		work.weights.push(weight.parse().unwrap());
		work.lanes.push(
			lanes
				.split(',')
				.filter(|g| *g != "-")
				.map(String::from)
				.collect(),
		);
	}
	assert_eq!(work.names.len(), 107);
	work.suite = work.names.iter().position(|n| n == SUITE).unwrap();
	work.parents = work
		.names
		.iter()
		.map(|name| {
			catalogue::prerequisites(name)
				.iter()
				.map(|p| work.names.iter().position(|n| n == p).unwrap())
				.collect()
		})
		.collect();
	work
}

impl Work {
	// The release is the end of exclusive ownership of target/debug only.
	fn check(&self, starts: &[u64], ends: &[u64], release: Option<u64>) -> Result<(), String> {
		for (i, name) in self.names.iter().enumerate() {
			for &p in &self.parents[i] {
				if starts[i] < ends[p] {
					return Err(format!("early consumer {name}"));
				}
			}
			for j in 0..i {
				if ends[i] <= starts[j] || ends[j] <= starts[i] {
					continue;
				}
				let lanes_at = |k: usize| {
					let mut lanes = self.lanes[k].clone();
					if k == self.suite && release.is_some_and(|t| starts[i].max(starts[j]) >= t) {
						lanes.remove(DEBUG);
					}
					lanes
				};
				if scheduler::conflict(&lanes_at(i), &lanes_at(j)) {
					return Err(format!("overlap {name} / {}", self.names[j]));
				}
			}
		}
		Ok(())
	}
	fn ends(&self) -> Vec<u64> {
		self.starts
			.iter()
			.zip(&self.durations)
			.map(|(s, d)| s + d)
			.collect()
	}
	fn overhead(&self) -> u64 {
		let mut spans: Vec<_> = self.starts.iter().copied().zip(self.ends()).collect();
		spans.sort_unstable();
		let mut end = 0;
		let mut busy = 0;
		for (s, e) in spans {
			busy += e.saturating_sub(s.max(end));
			end = end.max(e);
		}
		TOTAL - busy
	}
}

#[test]
fn published_intervals_cover_the_clock_and_respect_every_exclusion() {
	let work = witness();
	work.check(&work.starts, &work.ends(), None).unwrap();
	assert!(work.overhead().abs_diff(OUTSIDE_STAGES) < 100);
	let shared: u64 = work
		.lanes
		.iter()
		.zip(&work.durations)
		.filter(|(g, _)| g.contains(DEBUG))
		.map(|(_, d)| *d)
		.sum();
	assert!(shared.abs_diff(2_565_828_234) < 100);
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Event {
	Finish(usize),
	Release,
	Drain,
	ExclusiveEnd,
}

// Model the known exclusive-case acquisition explicitly. During its drain,
// returned tokens go to that request first. Otherwise stage leases have their
// recorded width and ordinary child loans are represented by the measured
// work durations. Vary the available width to expose sensitivity to loans.
fn predict(work: &Work, width: usize) -> (u64, Vec<u64>, Vec<u64>, Vec<String>) {
	let order = scheduler::admission_order(&work.names);
	let mut lanes = work.lanes.clone();
	let mut finished = BTreeSet::new();
	let mut running = BTreeSet::new();
	let mut events = BTreeSet::new();
	let mut starts = vec![0; order.len()];
	let mut ends = starts.clone();
	let mut links = vec![Vec::new(); order.len()];
	let mut cause = Vec::new();
	let mut drain = false;
	let mut exclusive = false;
	let mut now = 0;
	let suite = work.suite;
	let exclusive_at = (RECIPE_MS.iter().sum::<u64>() + CONTRACT_BUILD_MS) * 1000;
	while finished.len() < order.len() {
		if drain && running.len() == 1 {
			drain = false;
			exclusive = true;
			events.insert((now + EXCLUSIVE_WORK, Event::ExclusiveEnd));
			events.insert((
				now + work.durations[suite] - exclusive_at,
				Event::Finish(suite),
			));
			links[suite] = cause.clone();
			links[suite].push("exclusive contract".into());
		}
		if !drain && !exclusive {
			for (position, &i) in order.iter().enumerate() {
				if finished.contains(&i) || running.contains(&i) {
					continue;
				}
				let gate = scheduler::gate_after(
					i,
					order[..position].iter().copied(),
					&lanes,
					&finished,
					&running,
					&work.parents[i],
					&finished,
				);
				if gate.is_some() {
					continue;
				}
				let held: usize = running.iter().map(|j: &usize| work.weights[*j]).sum();
				if held + work.weights[i] > width {
					break;
				}
				running.insert(i);
				starts[i] = now;
				links[i] = cause.clone();
				links[i].push(work.names[i].clone());
				if i == suite {
					events.insert((now + RECIPE_MS[0] * 1000, Event::Release));
					events.insert((now + exclusive_at, Event::Drain));
				} else {
					events.insert((now + work.durations[i], Event::Finish(i)));
				}
			}
		}
		let (next, event) = events
			.pop_first()
			.expect("no event can unblock the schedule");
		now = next;
		match event {
			Event::Finish(i) => {
				finished.insert(i);
				running.remove(&i);
				ends[i] = now;
				cause = links[i].clone();
			}
			Event::Release => {
				assert!(lanes[suite].remove(DEBUG));
				cause = links[suite].clone();
				cause.push("debug lane release".into());
			}
			Event::Drain => {
				drain = true;
				cause = links[suite].clone();
			}
			Event::ExclusiveEnd => {
				exclusive = false;
				cause = links[suite].clone();
			}
		}
	}
	(now + OUTSIDE_STAGES, starts, ends, cause)
}

#[test]
fn counterfactual_reports_the_chain_and_token_sensitivity() {
	let work = witness();
	for width in [9, 11, 12, 13] {
		let (time, starts, ends, chain) = predict(&work, width);
		let release = starts[work.suite] + RECIPE_MS[0] * 1000;
		work.check(&starts, &ends, Some(release)).unwrap();
		println!(
			"{width} tokens: {:.3} s; finish chain {}",
			time as f64 / SECOND as f64,
			chain.join(" -> ")
		);
		if width == 13 {
			assert!(time <= 3000 * SECOND);
			let writer = (0..starts.len())
				.find(|&i| {
					i != work.suite
						&& work.lanes[i].contains(DEBUG)
						&& starts[i] >= release
						&& starts[i] < ends[work.suite]
				})
				.unwrap();
			let mut planted = starts.clone();
			planted[writer] = release - 1;
			assert!(
				work.check(&planted, &ends, Some(release))
					.unwrap_err()
					.starts_with("overlap")
			);
			let engine = work.names.iter().position(|s| s == "rune-clippy").unwrap();
			let mut planted = starts.clone();
			planted[engine] = release + 1;
			assert!(
				work.check(&planted, &ends, Some(release))
					.unwrap_err()
					.starts_with("overlap")
			);
			let consumer = work
				.names
				.iter()
				.position(|s| s == "production-cli-controls")
				.unwrap();
			let producer = work.parents[consumer][0];
			let mut planted = starts;
			planted[consumer] = ends[producer] - 1;
			assert!(
				work.check(&planted, &ends, Some(release))
					.unwrap_err()
					.starts_with("early consumer")
			);
		}
	}
}
