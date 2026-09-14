//! Own two inherited control pipes, verifying direction and revoking inheritance.
use std::{fs::File, io};

pub fn open(read: usize, write: usize) -> io::Result<(File, File)> {
	if read == write {
		return Err(io::Error::other("control endpoints must differ"));
	}
	validate(read, true)?;
	validate(write, false)?;
	// Do not take ownership until both endpoints pass validation.
	noninherit(read)?;
	noninherit(write)?;
	Ok(unsafe { (own(read), own(write)) })
}
#[cfg(unix)]
fn validate(n: usize, read: bool) -> io::Result<()> {
	if n <= 2 || n > i32::MAX as usize {
		return Err(io::Error::other("invalid control descriptor"));
	}
	let flags = unsafe { libc::fcntl(n as i32, libc::F_GETFL) };
	if flags < 0 {
		return Err(io::Error::last_os_error());
	}
	let mut stat = std::mem::MaybeUninit::uninit();
	if unsafe { libc::fstat(n as i32, stat.as_mut_ptr()) } < 0 {
		return Err(io::Error::last_os_error());
	}
	let stat = unsafe { stat.assume_init() };
	for standard in 0..=2 {
		let mut other = std::mem::MaybeUninit::uninit();
		if unsafe { libc::fstat(standard, other.as_mut_ptr()) } == 0 {
			let other = unsafe { other.assume_init() };
			if stat.st_dev == other.st_dev && stat.st_ino == other.st_ino {
				return Err(io::Error::other(
					"control endpoint aliases a standard stream",
				));
			}
		}
	}
	if stat.st_mode & libc::S_IFMT != libc::S_IFIFO
		|| flags & libc::O_ACCMODE != if read { libc::O_RDONLY } else { libc::O_WRONLY }
		|| flags & libc::O_NONBLOCK != 0
	{
		return Err(io::Error::other(
			"control endpoint is not a blocking pipe in the declared direction",
		));
	}
	Ok(())
}
#[cfg(unix)]
fn noninherit(n: usize) -> io::Result<()> {
	let flags = unsafe { libc::fcntl(n as i32, libc::F_GETFD) };
	if flags < 0 || unsafe { libc::fcntl(n as i32, libc::F_SETFD, flags | libc::FD_CLOEXEC) } < 0 {
		return Err(io::Error::last_os_error());
	}
	Ok(())
}
#[cfg(unix)]
unsafe fn own(n: usize) -> File {
	use std::os::fd::FromRawFd;
	unsafe { File::from_raw_fd(n as i32) }
}
#[cfg(windows)]
fn validate(n: usize, read: bool) -> io::Result<()> {
	use std::os::windows::io::AsRawHandle;
	use windows_sys::Wdk::Storage::FileSystem::*;
	use windows_sys::Win32::{Foundation::*, Storage::FileSystem::*, System::IO::*};
	let h = n as HANDLE;
	if h.is_null()
		|| h == INVALID_HANDLE_VALUE
		|| [
			std::io::stdin().as_raw_handle(),
			std::io::stdout().as_raw_handle(),
			std::io::stderr().as_raw_handle(),
		]
		.iter()
		.any(|standard| *standard == h || unsafe { CompareObjectHandles(*standard, h) } != 0)
	{
		return Err(io::Error::other("invalid control handle"));
	}
	if unsafe { GetFileType(h) } != FILE_TYPE_PIPE {
		return Err(io::Error::other("control handle is not a pipe"));
	}
	let mut status: IO_STATUS_BLOCK = unsafe { std::mem::zeroed() };
	let mut access = FILE_ACCESS_INFORMATION { AccessFlags: 0 };
	let result = unsafe {
		NtQueryInformationFile(
			h,
			&mut status,
			(&mut access as *mut FILE_ACCESS_INFORMATION).cast(),
			std::mem::size_of_val(&access) as u32,
			FileAccessInformation,
		)
	};
	if result < 0 {
		return Err(io::Error::other(format!(
			"cannot query control access: NTSTATUS {result:#x}"
		)));
	}
	let expected = if read {
		FILE_READ_DATA
	} else {
		FILE_WRITE_DATA
	};
	if access.AccessFlags & expected == 0 {
		return Err(io::Error::other("control handle has the wrong direction"));
	}
	Ok(())
}
#[cfg(windows)]
fn noninherit(n: usize) -> io::Result<()> {
	use windows_sys::Win32::Foundation::*;
	if unsafe { SetHandleInformation(n as HANDLE, HANDLE_FLAG_INHERIT, 0) } == 0 {
		return Err(io::Error::last_os_error());
	}
	Ok(())
}
#[cfg(windows)]
unsafe fn own(n: usize) -> File {
	use std::os::windows::io::FromRawHandle;
	unsafe { File::from_raw_handle(n as _) }
}
