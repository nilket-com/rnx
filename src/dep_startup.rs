//! Private readiness endpoint, sealed before settings or trusted builders run.
#[cfg(unix)]
pub(crate) struct Probe {
	socket: std::os::unix::net::UnixStream,
	engine: Option<crate::dep_wire::EngineChoice>,
}
#[cfg(not(unix))]
pub(crate) struct Probe;
impl Probe {
	pub(crate) fn take() -> crate::Result<Option<Self>> {
		let Some(raw) = std::env::var_os("RNX_INTERNAL_STARTUP_FD") else {
			return Ok(None);
		};
		#[cfg(unix)]
		{
			use std::os::fd::FromRawFd;
			let fd = raw
				.to_str()
				.ok_or("invalid startup descriptor")?
				.parse::<i32>()?;
			if fd < 3 {
				return Err("invalid startup descriptor".into());
			}
			if unsafe { libc::fcntl(fd, libc::F_SETFD, libc::FD_CLOEXEC) } < 0 {
				return Err(std::io::Error::last_os_error().into());
			}
			let socket = unsafe { std::os::unix::net::UnixStream::from_raw_fd(fd) };
			socket.set_write_timeout(Some(std::time::Duration::from_secs(1)))?;
			let engine = std::env::args()
				.find_map(|a| a.strip_prefix("--engine=").map(str::to_owned))
				.map(|s| {
					crate::dep_wire::EngineChoice::parse(if s == "auto" { "legacy" } else { &s })
				})
				.transpose()?;
			Ok(Some(Self { socket, engine }))
		}
		#[cfg(not(unix))]
		{
			let _ = raw;
			Err("startup probes are unsupported on this platform".into())
		}
	}
	pub(crate) fn finish(self, result: crate::Result<()>) -> crate::Result<()> {
		result?;
		#[cfg(unix)]
		{
			let mut socket = self.socket;
			let mut fields = crate::dep_wire::Fields::from([(1, "42".into())]);
			if let Some(engine) = self.engine {
				fields.insert(2, engine.name().into());
			}
			crate::dep_wire::write(&mut socket, 6, &fields)?;
		}
		Ok(())
	}
}
