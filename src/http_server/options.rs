use std::{net::SocketAddr, path::PathBuf, time::Duration};

pub(super) const HELP: &str = "rnx serve [--bind IP:PORT] [--workers N] [--budget N]\n          [--request-timeout-ms N] [--grace-ms N] [--log requests|off] PROGRAM.rn\n\nDefaults: 127.0.0.1:3000, 2 workers, 2000000 instructions, 30000 ms request\ntimeout, 5000 ms shutdown grace, request logging. Explicit remote binding is\nfor a reverse proxy; this host speaks HTTP/1 without TLS.\n\nOptional pub fn routes() returns [(METHOD, PATTERN, HANDLER_NAME)].\nNamed hooks not_found, method_not_allowed, bad_request are reserved.\nSee the HTTP handlers section in README.md for matching and parameter rules.\n";
#[derive(Debug)]
pub(super) struct Options {
	pub program: PathBuf,
	pub bind: SocketAddr,
	pub workers: usize,
	pub budget: usize,
	pub timeout: Duration,
	pub grace: Duration,
	pub request_logs: bool,
}
impl Options {
	pub fn parse(args: &[String]) -> Result<Self, String> {
		let mut out = Self {
			program: PathBuf::new(),
			bind: "127.0.0.1:3000".parse().unwrap(),
			workers: 2,
			budget: crate::runner::BUDGET,
			timeout: Duration::from_millis(30_000),
			grace: Duration::from_millis(5_000),
			request_logs: true,
		};
		let mut seen = std::collections::HashSet::new();
		let mut i = 0;
		while i < args.len() {
			let flag = &args[i];
			if !flag.starts_with('-') {
				if i + 1 != args.len() {
					return Err("serve takes one program path, after its options".into());
				}
				out.program = flag.into();
				return Ok(out);
			}
			if !matches!(
				flag.as_str(),
				"--bind"
					| "--workers" | "--budget"
					| "--request-timeout-ms"
					| "--grace-ms" | "--log"
			) {
				return Err(format!(
					"unknown serve option `{}`",
					crate::format::terminal_safe(flag)
				));
			}
			if !seen.insert(flag) {
				return Err(format!("serve option {flag} was given twice"));
			}
			let value = args
				.get(i + 1)
				.ok_or_else(|| format!("serve option {flag} needs a value"))?;
			let number = |max: usize| -> Result<usize, String> {
				let n = value
					.parse::<usize>()
					.ok()
					.filter(|n| (1..=max).contains(n));
				n.ok_or_else(|| format!("serve {flag} wants 1 to {max}"))
			};
			match flag.as_str() {
				"--bind" => {
					out.bind = value
						.parse()
						.map_err(|_| "serve --bind wants an IP:PORT socket address".to_owned())?
				}
				"--workers" => out.workers = number(16)?,
				"--budget" => out.budget = number(crate::runner::LARGEST_BUDGET)?,
				"--request-timeout-ms" => {
					out.timeout = Duration::from_millis(number(300_000)? as u64)
				}
				"--grace-ms" => out.grace = Duration::from_millis(number(300_000)? as u64),
				"--log" => {
					out.request_logs = match value.as_str() {
						"requests" => true,
						"off" => false,
						_ => return Err("serve --log wants requests or off".into()),
					}
				}
				_ => unreachable!(),
			}
			i += 2;
		}
		Err("serve needs a program path".into())
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	fn args(s: &[&str]) -> Vec<String> {
		s.iter().map(|s| s.to_string()).collect()
	}
	#[test]
	fn options_are_closed_before_source_opening() {
		for a in [
			vec![],
			vec!["--wat", "absent.rn"],
			vec!["--workers", "0", "absent.rn"],
			vec!["--workers", "17", "absent.rn"],
			vec!["--budget", "0", "absent.rn"],
			vec!["--budget", "18446744073709551615", "absent.rn"],
			vec!["--bind", "localhost:1", "absent.rn"],
			vec!["--log", "quiet", "absent.rn"],
			vec!["--log", "off", "--log", "off", "absent.rn"],
			vec!["--grace-ms"],
			vec!["a.rn", "b.rn"],
			vec!["a.rn", "--log", "off"],
		] {
			assert!(Options::parse(&args(&a)).is_err(), "{a:?}");
		}
		let o = Options::parse(&args(&["--bind", "[::1]:0", "--log", "off", "a.rn"])).unwrap();
		assert_eq!(o.bind.port(), 0);
		assert!(o.bind.is_ipv6());
		assert!(!o.request_logs);
		assert_eq!(o.budget, 2_000_000);
	}
}
