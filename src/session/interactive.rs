//! Sticky interactive engine choice shared by terminal and notebook sessions.
use super::{Failure, Session, domain_session::DomainSession};
pub(crate) use crate::dep_wire::EngineChoice;
pub(crate) enum InteractiveSession {
	Legacy(Box<Session>),
	Execution(Box<DomainSession>),
}
pub(crate) struct WorkerText {
	pub plain: String,
	pub html: Option<String>,
}
impl InteractiveSession {
	pub(crate) fn new(
		context: rune::Context,
		ceiling: usize,
		engine: EngineChoice,
		http: crate::http::State,
		lifecycle: crate::lifecycle::Lifecycle,
		presenters: crate::present::Presenters,
	) -> crate::Result<Self> {
		if engine == EngineChoice::Execution {
			Ok(Self::Execution(Box::new(
				DomainSession::new(context, ceiling)?
					.with_http(http)
					.with_lifecycle(lifecycle)
					.with_presenters(presenters),
			)))
		} else {
			Ok(Self::Legacy(Box::new(
				Session::with_ceiling(context, ceiling)?
					.with_http(http)
					.with_lifecycle(lifecycle)
					.with_presenters(presenters),
			)))
		}
	}
	pub(crate) fn eval_worker(
		&mut self,
		input: &str,
		armed: &mut dyn FnMut(usize) -> std::io::Result<()>,
	) -> std::io::Result<Result<Option<WorkerText>, Failure>> {
		let limits = crate::format::Limits::default();
		match self {
			Self::Execution(s) => Ok(s
				.eval_rendered_with_armed(input, &limits, armed)?
				.map(|text| text.map(|plain| WorkerText { plain, html: None }))),
			Self::Legacy(s) => Ok(s.eval_with_armed(input, armed)?.map(|value| {
				if crate::runner::is_unit(&value) {
					return None;
				}
				let plain = match s.present(&value, limits.total_bytes) {
					Some(outcome) => crate::present::finish(
						outcome,
						&crate::format::render(&value, None, &limits),
						limits.total_bytes,
					),
					None => crate::format::render(&value, Some(&s.fields()), &limits),
				};
				let html = s.present_html(&value);
				Some(WorkerText { plain, html })
			})),
		}
	}
	pub(crate) fn take_cleanup_notes(&mut self) -> Vec<String> {
		match self {
			Self::Legacy(_) => Vec::new(),
			Self::Execution(s) => s.take_cleanup_notes(),
		}
	}
	pub(crate) fn report_cleanup(&mut self) {
		if let Self::Execution(s) = self {
			s.report_cleanup();
		}
	}

	pub(crate) fn is_execution(&self) -> bool {
		self.engine() == EngineChoice::Execution
	}
	pub(crate) fn engine(&self) -> EngineChoice {
		match self {
			Self::Legacy(_) => EngineChoice::Legacy,
			Self::Execution(_) => EngineChoice::Execution,
		}
	}
	pub(crate) fn eval_rendered(
		&mut self,
		input: &str,
		limits: &crate::format::Limits,
	) -> Result<Option<String>, Failure> {
		match self {
			Self::Execution(s) => s.eval_rendered(input, limits),
			Self::Legacy(s) => {
				let value = s.eval(input)?;
				let text = match s.present(&value, limits.total_bytes) {
					Some(outcome) => crate::present::finish(
						outcome,
						&crate::format::render(&value, None, limits),
						limits.total_bytes,
					),
					None => crate::format::render_styled(
						&value,
						Some(&s.fields()),
						limits,
						crate::presentation::stdout(),
					),
				};
				Ok((!crate::runner::is_unit(&value)).then_some(text))
			}
		}
	}
	pub(crate) fn vars(&self, limits: &crate::inspect::InspectLimits) -> String {
		match self {
			Self::Legacy(s) => crate::inspect::vars(s, limits),
			Self::Execution(s) => crate::inspect::domain_vars(s, limits).0,
		}
	}
	pub(crate) fn help(
		&self,
		host: &[crate::host::HostFunction],
		commands: &[(&str, &str)],
		name: Option<&str>,
		limits: &crate::inspect::InspectLimits,
	) -> String {
		match self {
			Self::Legacy(s) => crate::inspect::help(s, host, commands, name, limits),
			Self::Execution(s) => crate::inspect::domain_help(s, host, commands, name, limits).0,
		}
	}
	pub(crate) fn next_number(&self) -> usize {
		match self {
			Self::Legacy(s) => s.next_number(),
			Self::Execution(s) => s.next_number(),
		}
	}
	pub(crate) fn last_generated(&self) -> &str {
		match self {
			Self::Legacy(s) => s.last_generated(),
			Self::Execution(s) => s.last_generated(),
		}
	}
	pub(crate) fn ceiling(&self) -> usize {
		match self {
			Self::Legacy(s) => s.ceiling(),
			Self::Execution(s) => s.ceiling(),
		}
	}
	pub(crate) fn over_ceiling(&self) -> bool {
		match self {
			Self::Legacy(s) => s.over_ceiling(),
			Self::Execution(s) => s.over_ceiling(),
		}
	}
	pub(crate) fn retained_bytes(&self) -> usize {
		match self {
			Self::Legacy(s) => s.retained_bytes(),
			Self::Execution(s) => s.retained_bytes(),
		}
	}
	pub(crate) fn retained_units(&self) -> usize {
		match self {
			Self::Legacy(s) => s.retained_units(),
			Self::Execution(s) => s.retained_units(),
		}
	}
	pub(crate) fn binding_names(&self) -> Vec<String> {
		match self {
			Self::Legacy(s) => s.binding_names(),
			Self::Execution(s) => s.binding_names(),
		}
	}
	pub(crate) fn declaration_names(&self) -> Vec<String> {
		match self {
			Self::Legacy(s) => s.declaration_names(),
			Self::Execution(s) => s.declaration_names(),
		}
	}
	pub(crate) fn reset_fallible(&mut self) -> Result<(), String> {
		match self {
			Self::Legacy(s) => s.reset_fallible(),
			Self::Execution(s) => s.reset_fallible(),
		}
	}
	pub(crate) fn lifecycle_failed(&self) -> bool {
		match self {
			Self::Legacy(s) => s.lifecycle_failed(),
			Self::Execution(s) => s.lifecycle_failed(),
		}
	}
	pub(crate) fn sample(&mut self) -> Option<usize> {
		match self {
			Self::Legacy(s) => s.sample(),
			Self::Execution(s) => s.sample(),
		}
	}
	pub(crate) fn close(&mut self) -> Result<(), String> {
		match self {
			Self::Legacy(s) => s.close(),
			Self::Execution(s) => s.close(),
		}
	}
	pub(crate) fn renumber(&mut self) {
		match self {
			Self::Legacy(s) => s.renumber(),
			Self::Execution(s) => s.renumber(),
		}
	}
	pub(crate) fn reset(&mut self) {
		match self {
			Self::Legacy(s) => s.reset(),
			Self::Execution(s) => s.reset(),
		}
	}
}
