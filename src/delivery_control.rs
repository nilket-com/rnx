//! Windows integration-test observations for the real host delivery worker.
//! Compiled only with test-support. The ordinary delivery has no file hooks.
use std::path::PathBuf;
use std::time::{Duration, Instant};

fn directory() -> Option<PathBuf> {
	std::env::var_os("RNX_TEST_DELIVERY_CONTROL").map(PathBuf::from)
}

/// Publish from the parent, so the writer's only I/O is its actual pipe write.
/// The gate retains Windows handles before triggering the interrupt.
pub fn announce<T>(worker: &std::thread::JoinHandle<T>, child: u32, bytes: usize) {
	use std::os::windows::io::AsRawHandle;
	use windows_sys::Win32::System::Threading::GetThreadId;
	let Some(dir) = directory() else { return };
	// SAFETY: the JoinHandle owns a live thread handle throughout this call.
	let thread = unsafe { GetThreadId(worker.as_raw_handle()) };
	if thread == 0 {
		eprintln!("rnx test-support: cannot identify the delivery thread");
		return;
	}
	// Rename after writing so the gate never reads a partial identity record.
	let result = std::fs::write(
		dir.join("identity.tmp"),
		format!("{thread} {child} {bytes}"),
	)
	.and_then(|_| std::fs::rename(dir.join("identity.tmp"), dir.join("identity")));
	if let Err(error) = result {
		eprintln!("rnx test-support: cannot announce delivery: {error}");
	}
}

/// What a gate is waiting to see before the worker is held.
///
/// An interrupt arrives independently of the delivery, so that hold waits for
/// the receipt before it begins. A deadline is the delivery's **own** ending:
/// there is nothing arriving from elsewhere to wait for, and waiting for an
/// interrupt that is never coming would hold every such run for the receipt's
/// whole bound and then not hold it at all.
/// A normal exit likewise needs no interrupt; `exit` names that separate
/// fixture premise without pretending a deadline caused it.
#[derive(PartialEq, Eq)]
enum Hold {
	Interrupt,
	Deadline,
	Exit,
}

fn hold_after() -> Hold {
	match std::env::var("RNX_TEST_DELIVERY_HOLD").as_deref() {
		Ok("deadline") => Hold::Deadline,
		Ok("exit") => Hold::Exit,
		_ => Hold::Interrupt,
	}
}

/// Keep the worker alive after delivery ends. A call that omits its join
/// can report a result while this marker exists and the worker is held.
pub fn before_thread_exit() {
	let Some(dir) = directory() else { return };
	if hold_after() == Hold::Interrupt {
		// Before the interrupt the external gate queries this thread's pending
		// I/O. A delivery that ended early must not substitute a filesystem write
		// for the pipe operation the gate is looking for. No I/O on this path.
		// Console events are delivered independently: the child can close its
		// pipe just before rnx's handler runs. Wait in memory for that receipt,
		// without creating another I/O operation the pending-I/O check could see.
		let receipt_until = Instant::now() + Duration::from_secs(2);
		while !crate::platform::interrupted() && Instant::now() < receipt_until {
			std::thread::sleep(Duration::from_millis(5));
		}
		if !crate::platform::interrupted() {
			return;
		}
	}
	if let Err(error) = std::fs::write(dir.join("finished-delivery"), "delivery ended") {
		eprintln!("rnx test-support: cannot announce delivery completion: {error}");
		return;
	}
	let until = Instant::now() + Duration::from_secs(20);
	while !dir.join("release-worker").exists() && Instant::now() < until {
		std::thread::sleep(Duration::from_millis(5));
	}
	if !dir.join("release-worker").exists() {
		eprintln!("rnx test-support: delivery worker was never released");
	}
}
