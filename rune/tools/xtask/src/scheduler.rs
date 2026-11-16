//! The jobserver budget lives in `job-budget`; this command's children
//! inherit it unchanged.
pub use job_budget::pipe::*;

#[cfg(test)]
mod tests {
	use super::*;
	use std::process::Command;
	#[test]
	fn child_shares_the_parent_jobserver() {
		let budget = Budget::new(2).unwrap();
		let _implicit = budget.acquire("cargo", 1).unwrap();
		let mut command = Command::new(std::env::current_exe().unwrap());
		command
			.args([
				"--exact",
				"scheduler::tests::jobserver_child",
				"--ignored",
				"--nocapture",
				"--test-threads=1",
			])
			.env("V4_JOBSERVER_CHILD", "1");
		budget.configure(&mut command);
		let output = command.output().unwrap();
		assert!(
			output.status.success(),
			"{}",
			String::from_utf8_lossy(&output.stderr)
		);
		assert!(String::from_utf8(output.stdout)
			.unwrap()
			.contains("SHARED_JOBSERVER_OK"));
	}
	#[test]
	#[ignore = "internal process-level jobserver probe"]
	fn jobserver_child() {
		assert_eq!(std::env::var("V4_JOBSERVER_CHILD").unwrap(), "1");
		// Only the descriptor set explicitly inherited by this test's parent is read.
		let client = unsafe { jobserver::Client::from_env() }.expect("shared jobserver");
		let _extra = client.acquire().unwrap();
		assert_eq!(client.available().unwrap(), 0);
		println!("SHARED_JOBSERVER_OK");
		if std::env::var_os("V4_EXIT_WITH_TOKEN").is_some() {
			std::process::exit(7);
		}
	}
}
