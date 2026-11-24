//! The final's receipts as declared types. Every receipt the controller
//! writes passes through its type, and every receipt it reads is parsed
//! into one: a missing, misspelled, mistyped or unexpected field is an
//! error at that boundary. Stage programs' own semantic receipts stay
//! opaque JSON; they are the stages', not the controller's.
use crate::evidence::Root;
use crate::receipt::{Field, atomic_json};
use anyhow::{Context as _, Result};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::{Number, Value};
use std::collections::BTreeMap;
use std::path::Path;

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum Status {
	#[serde(rename = "PASS")]
	Pass,
	#[serde(rename = "FAIL")]
	Fail,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Scope {
	Required,
	Complete,
	Explicit,
	Docs,
}

impl Scope {
	pub fn name(self) -> &'static str {
		match self {
			Scope::Required => "required",
			Scope::Complete => "complete",
			Scope::Explicit => "explicit",
			Scope::Docs => "docs",
		}
	}
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Phase {
	Plan,
	Impl,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
	Producer,
	Leaf,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Record {
	pub candidate: String,
	pub baseline: String,
	pub plan: String,
	pub record: String,
	pub phase: Phase,
	pub paths: Vec<String>,
	pub required: Vec<String>,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum FixPhase {
	Fix,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Fix {
	pub candidate: String,
	pub baseline: String,
	pub subject: String,
	pub phase: FixPhase,
	pub paths: Vec<String>,
	pub required: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(untagged)]
pub enum Candidate {
	Numbered(Record),
	Fix(Fix),
}

impl Candidate {
	pub fn is_publishable(&self) -> bool {
		match self {
			Self::Numbered(record) => record.phase == Phase::Impl,
			Self::Fix(_) => true,
		}
	}
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Selection {
	pub schema: u64,
	pub scope: Scope,
	pub publication_eligible: bool,
	pub record: Candidate,
	pub policy: String,
	pub source: String,
	pub root: String,
	pub out: String,
	pub stages: Vec<(String, Vec<String>)>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct EnvironmentIdentity {
	pub keys: Vec<String>,
	pub sha256: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ChildContext {
	pub schema: u64,
	pub invocation: String,
	pub root: String,
	pub cwd: String,
	pub command: Vec<String>,
	pub candidate: Option<String>,
	pub source: String,
	pub environment: EnvironmentIdentity,
	pub kind: Kind,
	#[serde(default, skip_serializing_if = "Field::is_absent")]
	pub result_directory: Field<String>,
	#[serde(default, skip_serializing_if = "Field::is_absent")]
	pub existing_results: Field<BTreeMap<String, String>>,
	#[serde(default, skip_serializing_if = "Field::is_absent")]
	pub resume: Field<bool>,
	/// The executable a Rust stage ran as: the controller's own.
	#[serde(default, skip_serializing_if = "Field::is_absent")]
	pub executable: Field<Executable>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Executable {
	pub path: String,
	pub sha256: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ProducerBinding {
	pub schema: u64,
	pub context: String,
	pub invocation: String,
	pub source: String,
	pub status: Status,
	pub receipt: String,
	pub sha256: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct FailureRef {
	pub path: String,
	pub sha256: String,
	pub stage: String,
	pub error: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct DeclaredOutput {
	pub output: String,
	pub archive: String,
	pub original_sha256: String,
	pub actual_sha256: String,
	pub verified: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct CleanupEvent {
	pub path: String,
	pub seconds: f64,
	pub sha256: String,
	pub status: Status,
}

/// A stage child's validation receipt (`<stage>-child/RESULT.json`).
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ChildResult {
	pub schema: u64,
	pub status: Status,
	pub command: Vec<String>,
	pub process_exit: Option<i32>,
	#[serde(default, skip_serializing_if = "Field::is_absent")]
	pub context: Field<ChildContext>,
	#[serde(default, skip_serializing_if = "Field::is_absent")]
	pub discovery: Field<Vec<String>>,
	#[serde(default, skip_serializing_if = "Field::is_absent")]
	pub test_count: Field<u64>,
	#[serde(default, skip_serializing_if = "Field::is_absent")]
	pub declared_output: Field<DeclaredOutput>,
	#[serde(default, skip_serializing_if = "Field::is_absent")]
	pub producer: Field<ProducerBinding>,
	#[serde(default, skip_serializing_if = "Field::is_absent")]
	pub error: Field<String>,
	#[serde(default, skip_serializing_if = "Field::is_absent")]
	pub error_type: Field<String>,
	#[serde(default, skip_serializing_if = "Field::is_absent")]
	pub failure: Field<FailureRef>,
	#[serde(default, skip_serializing_if = "Field::is_absent")]
	pub failure_retention_error: Field<String>,
	#[serde(default, skip_serializing_if = "Field::is_absent")]
	pub cleanup_error: Field<String>,
	pub seconds: f64,
	pub cleanup_seconds: Number,
	pub cleanup: Vec<CleanupEvent>,
	pub artifacts: BTreeMap<String, String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Validation {
	pub path: String,
	pub sha256: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct CheckRow {
	pub name: String,
	pub exit: i32,
	pub validation: Option<Validation>,
	#[serde(default, skip_serializing_if = "Field::is_absent")]
	pub declared_output: Field<DeclaredOutput>,
	#[serde(default, skip_serializing_if = "Field::is_absent")]
	pub failure: Field<FailureRef>,
	#[serde(default, skip_serializing_if = "Field::is_absent")]
	pub failure_retention_error: Field<String>,
	#[serde(default, skip_serializing_if = "Field::is_absent")]
	pub cleanup_error: Field<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Disk {
	pub floor_bytes: u64,
	pub estimate_bytes: u64,
	pub binding: String,
	pub lowest_free_bytes: Option<u64>,
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub output_path: Option<String>,
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub output_estimate_bytes: Option<u64>,
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub output_lowest_free_bytes: Option<u64>,
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub scratch_path: Option<String>,
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub scratch_estimate_bytes: Option<u64>,
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub scratch_lowest_free_bytes: Option<u64>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Memory {
	pub tokens: Option<String>,
	pub events_before: Option<BTreeMap<String, u64>>,
	pub events_after: Option<BTreeMap<String, u64>>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Durations {
	pub seconds: Option<Number>,
	pub cleanup_seconds: Option<Number>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Finalization {
	pub schema: u64,
	pub status: Status,
	pub candidate: String,
	pub scope: Scope,
	pub publication_eligible: bool,
	pub source: String,
	pub selection: String,
	pub evidence: BTreeMap<String, Validation>,
	pub durations: BTreeMap<String, Durations>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Acceptance {
	pub status: Status,
	pub candidate: Option<String>,
	pub acceptance_scope: Scope,
	pub mode: String,
	#[serde(default, skip_serializing_if = "Field::is_absent")]
	pub error: Field<String>,
	pub checks: Vec<CheckRow>,
	pub seconds: f64,
	#[serde(default, skip_serializing_if = "Field::is_absent")]
	pub disk: Field<Disk>,
	pub memory: Memory,
	#[serde(default, skip_serializing_if = "Field::is_absent")]
	pub failed_stage: Field<String>,
	#[serde(default, skip_serializing_if = "Field::is_absent")]
	pub failure: Field<FailureRef>,
	#[serde(default, skip_serializing_if = "Field::is_absent")]
	pub finalization: Field<Finalization>,
	#[serde(default, skip_serializing_if = "Field::is_absent")]
	pub recovered: Field<Vec<String>>,
	#[serde(default, skip_serializing_if = "Field::is_absent")]
	pub manifest_error: Field<String>,
	#[serde(default, skip_serializing_if = "Field::is_absent")]
	pub diagnostics_error: Field<String>,
	/// The digest of the final's SCHEDULE.json, which revalidation then
	/// requires (finals before ADR 0288 have none).
	#[serde(default, skip_serializing_if = "Field::is_absent")]
	pub schedule_sha256: Field<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Payload {
	pub sha256: String,
	pub entries: u64,
	pub logical_bytes: u64,
	pub allocated_bytes: u64,
	pub reused_file_digests: u64,
	pub seconds: f64,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ManifestRoot {
	pub owner: Root,
	#[serde(default, skip_serializing_if = "Field::is_absent")]
	pub payload: Field<Payload>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ManifestStage {
	pub name: String,
	pub command_template: Vec<String>,
	pub status: Status,
	pub exit: Option<i32>,
	pub seconds: f64,
	pub cleanup_seconds: Number,
	pub environment_names: Vec<String>,
	pub receipt_sha256: String,
	pub artifact_set_sha256: String,
}

/// MANIFEST.json, or its incomplete form after a failed snapshot.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
	pub schema: u64,
	pub candidate: Option<String>,
	#[serde(default, skip_serializing_if = "Field::is_absent")]
	pub fix_subject: Field<String>,
	#[serde(default, skip_serializing_if = "Field::is_absent")]
	pub source: Field<String>,
	#[serde(default, skip_serializing_if = "Field::is_absent")]
	pub policy: Field<String>,
	#[serde(default, skip_serializing_if = "Field::is_absent")]
	pub acceptance_scope: Field<Scope>,
	#[serde(default, skip_serializing_if = "Field::is_absent")]
	pub selection_sha256: Field<String>,
	/// The controller's own identity; each controller writes its own.
	#[serde(default, skip_serializing_if = "Field::is_absent")]
	pub tools: Field<Value>,
	#[serde(default, skip_serializing_if = "Field::is_absent")]
	pub stages: Field<Vec<ManifestStage>>,
	pub roots: Vec<ManifestRoot>,
	#[serde(default, skip_serializing_if = "Field::is_absent")]
	pub incomplete: Field<bool>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Release {
	pub status: Status,
	#[serde(default, skip_serializing_if = "Field::is_absent")]
	pub ledger_sha256: Field<String>,
	#[serde(default, skip_serializing_if = "Field::is_absent")]
	pub roots: Field<Vec<Root>>,
	#[serde(default, skip_serializing_if = "Field::is_absent")]
	pub acceptance_sha256: Field<String>,
	#[serde(default, skip_serializing_if = "Field::is_absent")]
	pub manifest_sha256: Field<String>,
	#[serde(default, skip_serializing_if = "Field::is_absent")]
	pub error: Field<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ResultCheck {
	pub name: String,
	pub exit: i32,
}

/// RESULT.json: the final's verdict and its terminal records' digests.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct FinalResult {
	pub status: Status,
	pub candidate: Option<String>,
	#[serde(default, skip_serializing_if = "Field::is_absent")]
	pub fix_subject: Field<String>,
	pub seconds: f64,
	pub checks: Vec<ResultCheck>,
	pub error: Option<String>,
	pub release_error: Option<String>,
	pub manifest_error: Option<String>,
	pub diagnostics_error: Option<String>,
	pub finalization: Option<Finalization>,
	pub remote_context: Option<Value>,
	pub acceptance_scope: Scope,
	pub complete_final: bool,
	pub publication_eligible: bool,
	pub free_bytes_before: u64,
	pub free_bytes_after: u64,
	pub records: BTreeMap<String, String>,
	#[serde(default, skip_serializing_if = "Field::is_absent")]
	pub failed_stage: Field<String>,
	#[serde(default, skip_serializing_if = "Field::is_absent")]
	pub failure: Field<FailureRef>,
	#[serde(default, skip_serializing_if = "Field::is_absent")]
	pub retirement_error: Field<String>,
	#[serde(default, skip_serializing_if = "Field::is_absent")]
	pub wall_budget_error: Field<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Successor {
	pub status: Status,
	pub candidate: String,
	pub prior: String,
	pub prior_candidate: String,
	pub prior_result_sha256: String,
	pub changed: Vec<String>,
	pub reran: Vec<String>,
	pub plans_final: String,
	#[serde(default, skip_serializing_if = "Field::is_absent")]
	pub plans_final_result_sha256: Field<String>,
}

/// Read a receipt into its type, refusing any field the type lacks.
pub fn read<T: DeserializeOwned>(path: &Path) -> Result<T> {
	let bytes = std::fs::read(path).with_context(|| format!("read {}", path.display()))?;
	serde_json::from_slice(&bytes)
		.with_context(|| format!("{} does not match its receipt type", path.display()))
}

/// A JSON value as a receipt type, refusing what the type does not declare.
pub fn conform<T: DeserializeOwned>(value: &Value, what: &str) -> Result<T> {
	serde_json::from_value(value.clone())
		.with_context(|| format!("{what} does not match its receipt type"))
}

/// Write a receipt only once it is its type.
pub fn write_as<T: DeserializeOwned + Serialize>(path: &Path, value: &Value) -> Result<()> {
	let typed: T = conform(value, &path.display().to_string())?;
	atomic_json(path, &typed)
}
