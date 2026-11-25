//! What admission observed of each stage (ADR 0288): when its readiness
//! was established, each gate it was then refused at, and when it started
//! and ended; and when the controller blocked between passes and what woke
//! it. Only a change is recorded. Times are the controller's decision
//! times, seconds since the final began; a refusal says the gate refused
//! then, not that it would have refused until the next record.
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeSet;

/// The terminal record, as written and as revalidation reads it.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Schedule {
	pub schema: u64,
	pub stages: Vec<StageTrace>,
	pub controller: Vec<ControllerEvent>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct StageTrace {
	pub name: String,
	/// `waiting`, `running`, `passed`, `failed` or `not started`.
	pub state: String,
	/// When its producers, lanes and alone checks were first seen to pass.
	pub ready: Option<f64>,
	pub waits: Vec<Wait>,
	/// Controller observation after the last target/debug writer returned.
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub debug_lane_released: Option<f64>,
	pub started: Option<f64>,
	pub ended: Option<f64>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Wait {
	pub at: f64,
	/// `producers`, `lanes`, `alone`, `disk`, `tokens` or `queued`.
	pub gate: String,
	/// The stages it waited on: unpassed producers, the earlier stage
	/// sharing a lane, or the stage that cut the pass short.
	#[serde(default, skip_serializing_if = "Vec::is_empty")]
	pub behind: Vec<String>,
	/// The numbers the gate compared: free and needed bytes, or the weight.
	#[serde(default, skip_serializing_if = "Value::is_null")]
	pub evidence: Value,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ControllerEvent {
	pub at: f64,
	/// `blocked` (waiting for a stage to end or a signal) or `woken`.
	pub event: String,
	/// What woke it: a stage's name, `signal`, or `tokens` (a write to the
	/// jobserver while a stage waited on tokens).
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub by: Option<String>,
}

pub const GATES: [&str; 6] = ["producers", "lanes", "alone", "disk", "tokens", "queued"];
const STATES: [&str; 5] = ["waiting", "running", "passed", "failed", "not started"];

/// The record as admission writes it.
pub struct Recorder {
	schedule: Schedule,
}

impl Recorder {
	pub fn new(names: &[String]) -> Self {
		Self {
			schedule: Schedule {
				schema: 1,
				stages: names
					.iter()
					.map(|n| StageTrace {
						name: n.clone(),
						state: "waiting".into(),
						ready: None,
						waits: vec![],
						debug_lane_released: None,
						started: None,
						ended: None,
					})
					.collect(),
				controller: vec![],
			},
		}
	}

	/// The stage was refused at `gate`; written only when it differs from
	/// its last refusal (by gate and the stages it waited on).
	pub fn wait(
		&mut self,
		index: usize,
		at: f64,
		gate: &str,
		behind: Vec<String>,
		evidence: Value,
	) {
		let trace = &mut self.schedule.stages[index];
		if trace.started.is_some() {
			return;
		}
		if trace
			.waits
			.last()
			.is_some_and(|w| w.gate == gate && w.behind == behind)
		{
			return;
		}
		trace.waits.push(Wait {
			at,
			gate: gate.into(),
			behind,
			evidence,
		});
	}

	/// Its readiness checks passed; the first time only.
	pub fn ready(&mut self, index: usize, at: f64) {
		let trace = &mut self.schedule.stages[index];
		if trace.ready.is_none() {
			trace.ready = Some(at);
		}
	}

	pub fn start(&mut self, index: usize, at: f64) {
		let trace = &mut self.schedule.stages[index];
		trace.started = Some(at);
		trace.state = "running".into();
	}

	pub fn release_debug(&mut self, index: usize, at: f64) {
		self.schedule.stages[index].debug_lane_released = Some(at);
	}

	pub fn end(&mut self, index: usize, at: f64, passed: bool) {
		let trace = &mut self.schedule.stages[index];
		trace.ended = Some(at);
		trace.state = if passed { "passed" } else { "failed" }.into();
	}

	/// The controller blocked for an event, or was woken by one; a repeat
	/// of the last event is not written.
	pub fn controller(&mut self, at: f64, event: &str, by: Option<String>) {
		if event == "blocked"
			&& self
				.schedule
				.controller
				.last()
				.is_some_and(|e| e.event == "blocked")
		{
			return;
		}
		self.schedule.controller.push(ControllerEvent {
			at,
			event: event.into(),
			by,
		});
	}

	/// The record, a stage that never started marked so, with no end.
	pub fn finish(mut self) -> Schedule {
		for trace in &mut self.schedule.stages {
			if trace.started.is_none() {
				trace.state = "not started".into();
			}
		}
		self.schedule
	}
}

/// A schedule's own consistency, against the final's selected stages:
/// exactly those stages, known gates and states, finite nonnegative times
/// in order, and a state that agrees with its start and end.
pub fn validate(schedule: &Schedule, selected: &[String]) -> Result<()> {
	ensure!(schedule.schema == 1, "unknown schedule schema");
	let names: Vec<&String> = schedule.stages.iter().map(|s| &s.name).collect();
	ensure!(
		names == selected.iter().collect::<Vec<_>>(),
		"schedule stages differ from the selection"
	);
	let time = |t: f64| t.is_finite() && t >= 0.0;
	for s in &schedule.stages {
		ensure!(
			STATES.contains(&s.state.as_str()),
			"{}: unknown state {}",
			s.name,
			s.state
		);
		let mut last = 0.0;
		for w in &s.waits {
			ensure!(
				GATES.contains(&w.gate.as_str()),
				"{}: unknown gate {}",
				s.name,
				w.gate
			);
			ensure!(
				time(w.at) && w.at >= last,
				"{}: wait times out of order",
				s.name
			);
			last = w.at;
			let known: BTreeSet<&String> = selected.iter().collect();
			ensure!(
				w.behind.iter().all(|b| known.contains(b)),
				"{}: waits on an unknown stage",
				s.name
			);
		}
		for t in [s.ready, s.started, s.ended].into_iter().flatten() {
			ensure!(time(t), "{}: a time is not finite and nonnegative", s.name);
		}
		if let Some(start) = s.started {
			// The writer establishes readiness before it starts a stage.
			ensure!(
				s.ready.is_some_and(|r| r <= start),
				"{}: started without being ready first",
				s.name
			);
			ensure!(
				s.waits.last().is_none_or(|w| w.at <= start),
				"{}: waited after it started",
				s.name
			);
		}
		if let Some(at) = s.debug_lane_released {
			ensure!(
				s.name == "rune-suite"
					&& time(at) && s.started.is_some_and(|t| t <= at)
					&& s.ended.is_none_or(|t| at <= t),
				"invalid debug lane release"
			);
		}
		match (s.started, s.ended, s.state.as_str()) {
			(None, None, "not started" | "waiting") => {}
			(Some(_), None, "running") => {}
			(Some(start), Some(end), "passed" | "failed") => {
				ensure!(start <= end, "{}: ended before it started", s.name);
			}
			_ => anyhow::bail!(
				"{}: state {} disagrees with its start and end",
				s.name,
				s.state
			),
		}
	}
	let mut last = 0.0;
	for e in &schedule.controller {
		ensure!(
			matches!(e.event.as_str(), "blocked" | "woken"),
			"unknown controller event {}",
			e.event
		);
		ensure!(time(e.at) && e.at >= last, "controller events out of order");
		ensure!(
			e.by.as_ref()
				.is_none_or(|b| b == "signal" || b == "tokens" || selected.contains(b)),
			"the controller was woken by an unknown stage"
		);
		last = e.at;
	}
	Ok(())
}
