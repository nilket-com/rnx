//! Exclusive rename. No branch checks destination existence or falls back.
use std::io;
#[cfg(any(target_os = "linux", target_os = "macos"))]
fn rename_with(
	from: &str,
	to: &str,
	call: impl FnOnce(&std::ffi::CStr, &std::ffi::CStr, u32) -> i32,
) -> io::Result<()> {
	let from = std::ffi::CString::new(from)?;
	let to = std::ffi::CString::new(to)?;
	#[cfg(target_os = "linux")]
	let flags = libc::RENAME_NOREPLACE;
	#[cfg(target_os = "macos")]
	let flags = libc::RENAME_EXCL;
	if call(&from, &to, flags) == 0 {
		Ok(())
	} else {
		Err(io::Error::last_os_error())
	}
}
#[cfg(target_os = "linux")]
pub fn rename(from: &str, to: &str) -> io::Result<()> {
	rename_with(from, to, |a, b, flags| {
		// SAFETY: CStrings live through the syscall and flags request no replacement.
		unsafe {
			libc::renameat2(
				libc::AT_FDCWD,
				a.as_ptr(),
				libc::AT_FDCWD,
				b.as_ptr(),
				flags,
			)
		}
	})
}
#[cfg(target_os = "macos")]
pub fn rename(from: &str, to: &str) -> io::Result<()> {
	rename_with(from, to, |a, b, flags| unsafe {
		libc::renamex_np(a.as_ptr(), b.as_ptr(), flags)
	})
}
#[cfg(windows)]
fn rename_with(
	from: &str,
	to: &str,
	call: impl FnOnce(*const u16, *const u16, u32) -> i32,
) -> io::Result<()> {
	use std::os::windows::ffi::OsStrExt;
	fn wide(s: &str) -> io::Result<Vec<u16>> {
		let mut v: Vec<u16> = std::ffi::OsStr::new(s).encode_wide().collect();
		if v.contains(&0) {
			return Err(io::Error::new(
				io::ErrorKind::InvalidInput,
				"path contains a NUL",
			));
		}
		v.push(0);
		Ok(v)
	}
	let from = wide(from)?;
	let to = wide(to)?;
	if call(from.as_ptr(), to.as_ptr(), 0) != 0 {
		Ok(())
	} else {
		Err(io::Error::last_os_error())
	}
}
#[cfg(windows)]
pub fn rename(from: &str, to: &str) -> io::Result<()> {
	rename_with(from, to, |a, b, flags| {
		// SAFETY: terminated UTF-16 buffers live through the call. Flags exclude replacement and cross-volume copying.
		unsafe { windows_sys::Win32::Storage::FileSystem::MoveFileExW(a, b, flags) }
	})
}
#[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
pub fn rename(_from: &str, _to: &str) -> io::Result<()> {
	Err(io::Error::new(
		io::ErrorKind::Unsupported,
		"atomic no-replace rename is unsupported on this platform",
	))
}
#[cfg(test)]
mod tests {
	#[test]
	#[cfg(any(target_os = "linux", target_os = "macos"))]
	fn exclusive_call_needs_no_existing_source_or_destination() {
		super::rename_with(
			"/rnx-nonexistent-parent/source",
			"/rnx-nonexistent-parent/target",
			|from, to, flags| {
				assert_eq!(from.to_bytes(), b"/rnx-nonexistent-parent/source");
				assert_eq!(to.to_bytes(), b"/rnx-nonexistent-parent/target");
				#[cfg(target_os = "linux")]
				assert_eq!(flags, libc::RENAME_NOREPLACE);
				#[cfg(target_os = "macos")]
				assert_eq!(flags, libc::RENAME_EXCL);
				0
			},
		)
		.unwrap();
	}
	#[test]
	#[cfg(windows)]
	fn exclusive_call_needs_no_existing_source_or_destination() {
		super::rename_with(
			"Z:\\rnx-nonexistent-parent\\source",
			"Z:\\rnx-nonexistent-parent\\target",
			|_, _, flags| {
				assert_eq!(flags, 0);
				1
			},
		)
		.unwrap();
	}
}
