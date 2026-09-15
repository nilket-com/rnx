//! Linux worker supervision. No blocking pipe reader or unbounded output channel.
use crate::{
	containment,
	streams::{CAP, Capture, Text},
	transport::Res,
};
use serde_json::{Value, json};
use std::{
	future::Future,
	io,
	os::{
		fd::{AsRawFd, FromRawFd, OwnedFd},
		unix::process::CommandExt,
	},
	path::Path,
	process::{Child, Command, Stdio},
	sync::{Arc, Mutex},
};
use tokio::{
	io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader},
	net::unix::pipe::{Receiver, Sender},
	sync::{Notify, mpsc, watch},
	time::{Duration, Instant, timeout},
};

#[derive(Default)]
struct SignalState {
	active: bool,
	armed: bool,
	pending: bool,
	disabled: bool,
}
#[derive(Default)]
pub struct Interrupt {
	state: Mutex<SignalState>,
	pid: std::sync::atomic::AtomicI32,
}
impl Interrupt {
	pub fn set_pid(&self, pid: i32) {
		self.pid.store(pid, std::sync::atomic::Ordering::SeqCst);
	}
	pub fn begin(&self) {
		let mut s = self.state.lock().unwrap();
		s.active = true;
		s.armed = false;
	}
	pub fn request(&self, queued: bool) -> Res<()> {
		let mut s = self.state.lock().unwrap();
		if s.disabled {
			return Ok(());
		}
		if s.active || queued {
			s.pending = true;
			if s.armed {
				containment::signal(
					self.pid.load(std::sync::atomic::Ordering::SeqCst),
					libc::SIGINT,
				)?;
				s.pending = false;
			}
		}
		Ok(())
	}
	fn armed(&self) -> Res<()> {
		let mut s = self.state.lock().unwrap();
		s.armed = true;
		if s.pending && !s.disabled {
			containment::signal(
				self.pid.load(std::sync::atomic::Ordering::SeqCst),
				libc::SIGINT,
			)?;
			s.pending = false;
		}
		Ok(())
	}
	pub fn finish(&self) {
		let mut s = self.state.lock().unwrap();
		s.active = false;
		s.armed = false;
		s.pending = false;
	}
	pub fn disable(&self) {
		let mut s = self.state.lock().unwrap();
		s.disabled = true;
		s.active = false;
		s.armed = false;
		s.pending = false;
	}
}
struct ChildGuard {
	child: Child,
	kill: bool,
}
impl Drop for ChildGuard {
	fn drop(&mut self) {
		if self.kill {
			let p = self.child.id() as i32;
			let _ = containment::signal(-p, libc::SIGKILL);
			let _ = containment::signal(p, libc::SIGKILL);
		}
	}
}
#[derive(Default)]
struct Late {
	data: Vec<u8>,
	sent: usize,
	discarded: u64,
	notified: bool,
}
impl Late {
	fn feed(&mut self, b: &[u8]) -> Res<()> {
		let n = b.len().min(CAP - self.data.len());
		self.data.try_reserve_exact(n)?;
		self.data.extend_from_slice(&b[..n]);
		self.discarded = self
			.discarded
			.checked_add((b.len() - n) as u64)
			.ok_or("late output counter overflow")?;
		Ok(())
	}
}
#[derive(Default)]
struct Collected {
	active: Option<[Capture; 2]>,
	late: [Late; 2],
	error: Option<String>,
	eof: [bool; 2],
}
pub struct StreamEvent {
	pub name: &'static str,
	pub text: String,
	pub replaced: bool,
	pub unassociated: bool,
}
struct Control {
	value: Value,
}
pub struct Worker {
	child: ChildGuard,
	write: Sender,
	control: mpsc::Receiver<Control>,
	collected: Arc<Mutex<Collected>>,
	notify: Arc<Notify>,
	deadline: watch::Sender<Option<Instant>>,
	tasks: tokio::task::JoinSet<()>,
	id: u64,
	pub interrupt: Arc<Interrupt>,
	late_text: [Text; 2],
}
fn pipes() -> io::Result<(OwnedFd, OwnedFd)> {
	let mut fds = [0; 2];
	if unsafe { libc::pipe2(fds.as_mut_ptr(), libc::O_CLOEXEC) } < 0 {
		return Err(io::Error::last_os_error());
	}
	Ok(unsafe { (OwnedFd::from_raw_fd(fds[0]), OwnedFd::from_raw_fd(fds[1])) })
}
fn failure(
	shared: &Mutex<Collected>,
	notify: &Notify,
	deadline: &watch::Sender<Option<Instant>>,
	message: String,
) {
	let mut s = shared.lock().unwrap();
	if s.error.is_none() {
		s.error = Some(message);
	}
	deadline.send_if_modified(|end| {
		if end.is_none() {
			*end = Some(Instant::now() + Duration::from_secs(5));
			true
		} else {
			false
		}
	});
	notify.notify_one();
}
impl Worker {
	pub async fn spawn(binary: &Path, interrupt: Arc<Interrupt>) -> Res<Self> {
		if !binary.is_absolute() {
			return Err("--rnx needs an absolute executable path".into());
		}
		let (child_read, parent_write) = pipes()?;
		let (parent_read, child_write) = pipes()?;
		let r = child_read.as_raw_fd();
		let w = child_write.as_raw_fd();
		if r <= 2 || w <= 2 {
			return Err("worker control descriptors must be above stderr".into());
		}
		let mut cmd = Command::new(binary);
		cmd.args([
			"worker",
			"--control-read",
			&r.to_string(),
			"--control-write",
			&w.to_string(),
		]);
		cmd.stdin(Stdio::null())
			.stdout(Stdio::piped())
			.stderr(Stdio::piped());
		// All pipes start CLOEXEC. Only the child-side controls lose the flag in
		// the child, immediately before exec. rnx revokes inheritance on receipt.
		unsafe {
			cmd.pre_exec(move || {
				if libc::setpgid(0, 0) < 0 {
					return Err(io::Error::last_os_error());
				}
				for fd in [r, w] {
					let flags = libc::fcntl(fd, libc::F_GETFD);
					if flags < 0 || libc::fcntl(fd, libc::F_SETFD, flags & !libc::FD_CLOEXEC) < 0 {
						return Err(io::Error::last_os_error());
					}
				}
				Ok(())
			});
		}
		let mut child = ChildGuard {
			child: cmd
				.spawn()
				.map_err(|e| format!("cannot start worker {}: {e}", binary.display()))?,
			kill: true,
		};
		drop(child_read);
		drop(child_write);
		let stdout = Receiver::from_owned_fd(child.child.stdout.take().unwrap().into())?;
		let stderr = Receiver::from_owned_fd(child.child.stderr.take().unwrap().into())?;
		let write = Sender::from_owned_fd(parent_write)?;
		let read = Receiver::from_owned_fd(parent_read)?;
		interrupt.set_pid(child.child.id() as i32);
		let collected = Arc::new(Mutex::new(Collected::default()));
		let notify = Arc::new(Notify::new());
		let (deadline, _) = watch::channel(None);
		let (tx, control) = mpsc::channel(8);
		let mut tasks = tokio::task::JoinSet::new();
		for (index, mut read) in [stdout, stderr].into_iter().enumerate() {
			let shared = collected.clone();
			let notify = notify.clone();
			let deadline = deadline.clone();
			tasks.spawn(async move {
				let mut b = [0; 8192];
				loop {
					match read.read(&mut b).await {
						Ok(0) => {
							shared.lock().unwrap().eof[index] = true;
							failure(&shared, &notify, &deadline, "worker stream closed".into());
							break;
						}
						Ok(n) => {
							let result = (|| -> Res<()> {
								let mut s = shared.lock().unwrap();
								let tail = if let Some(active) = &mut s.active {
									active[index].feed(&b[..n])?
								} else {
									b[..n].to_vec()
								};
								s.late[index].feed(&tail)
							})();
							if let Err(e) = result {
								failure(&shared, &notify, &deadline, e.to_string());
								break;
							}
							notify.notify_one();
						}
						Err(e) => {
							failure(&shared, &notify, &deadline, e.to_string());
							break;
						}
					}
				}
			});
		}
		let shared = collected.clone();
		let changed = notify.clone();
		let end = deadline.clone();
		tasks.spawn(async move {
			let mut reader = BufReader::new(read);
			loop {
				let result = async {
					let mut line = Vec::new();
					loop {
						let b = reader.fill_buf().await?;
						if b.is_empty() {
							return Err::<Value, Box<dyn std::error::Error + Send + Sync>>(
								"worker control closed".into(),
							);
						}
						let newline = b.iter().position(|b| *b == b'\n');
						let n = newline.unwrap_or(b.len());
						if line.len() + n > 262144 {
							return Err("worker control frame exceeds 256 KiB".into());
						}
						line.extend_from_slice(&b[..n]);
						reader.consume(n + usize::from(newline.is_some()));
						if newline.is_some() {
							return Ok(serde_json::from_slice(&line)?);
						}
					}
				}
				.await;
				match result {
					Ok(value) => {
						if value["type"] == "settled" {
							end.send_if_modified(|deadline| {
								if deadline.is_none() {
									*deadline = Some(Instant::now() + Duration::from_secs(5));
									true
								} else {
									false
								}
							});
						}
						if tx.send(Control { value }).await.is_err() {
							break;
						}
					}
					Err(e) => {
						failure(&shared, &changed, &end, e.to_string());
						break;
					}
				}
			}
		});
		let mut worker = Self {
			child,
			write,
			control,
			collected,
			notify,
			deadline,
			tasks,
			id: 0,
			interrupt,
			late_text: Default::default(),
		};
		let ready = timeout(Duration::from_secs(5), worker.control.recv())
			.await?
			.ok_or("worker died before ready")?
			.value;
		if ready["type"] != "ready"
			|| ready["protocol"] != 1
			|| ready["rnx"] != "0.0.0"
			|| ready["rune"] != "0.14.2"
		{
			return Err(format!("incompatible worker: expected protocol 1, rnx 0.0.0, Rune 0.14.2; received protocol {}, rnx {}, Rune {}",ready["protocol"],ready["rnx"],ready["rune"]).into());
		}
		Ok(worker)
	}
	pub fn notify(&self) -> Arc<Notify> {
		self.notify.clone()
	}
	pub fn health(&self) -> Res<()> {
		if let Some(e) = &self.collected.lock().unwrap().error {
			Err(e.clone().into())
		} else {
			Ok(())
		}
	}
	async fn send(&mut self, value: Value) -> Res<()> {
		let mut b = serde_json::to_vec(&value)?;
		if b.len() > 262144 {
			return Err("outgoing worker frame exceeds 256 KiB".into());
		}
		b.push(b'\n');
		self.write.write_all(&b).await?;
		Ok(())
	}
	/// The watchdog begins when the reader sees settled, including while an
	/// application sink is pending. It is not started after forwarding finishes.
	pub async fn operate<F, Fut, C, Done>(
		&mut self,
		source: Option<&str>,
		sink: &mut F,
		complete: &mut C,
	) -> Res<Value>
	where
		F: FnMut(StreamEvent) -> Fut,
		Fut: Future<Output = Res<()>>,
		C: FnMut(Value) -> Done,
		Done: Future<Output = Res<()>>,
	{
		self.deadline.send_replace(None);
		let mut deadline = self.deadline.subscribe();
		let watchdog = async move {
			loop {
				let end = *deadline.borrow_and_update();
				if let Some(end) = end {
					tokio::time::sleep_until(end).await;
					break;
				}
				if deadline.changed().await.is_err() {
					break;
				}
			}
		};
		tokio::select! {r=self.operation(source,sink,complete)=>r,_=watchdog=>Err("worker settlement/publication deadline exceeded; no acknowledgement".into())}
	}
	async fn operation<F, Fut, C, Done>(
		&mut self,
		source: Option<&str>,
		sink: &mut F,
		complete: &mut C,
	) -> Res<Value>
	where
		F: FnMut(StreamEvent) -> Fut,
		Fut: Future<Output = Res<()>>,
		C: FnMut(Value) -> Done,
		Done: Future<Output = Res<()>>,
	{
		self.health()?;
		self.id = self
			.id
			.checked_add(1)
			.filter(|n| *n < 1 << 53)
			.ok_or("worker request identity exhausted")?;
		let nonce = format!(
			"{}{}",
			uuid::Uuid::new_v4().simple(),
			uuid::Uuid::new_v4().simple()
		);
		{
			let mut s = self.collected.lock().unwrap();
			if s.active.is_some() {
				return Err("previous worker boundary was not acknowledged".into());
			}
			s.active = Some([
				Capture::new(self.id, "stdout", &nonce),
				Capture::new(self.id, "stderr", &nonce),
			]);
		}
		let request = if let Some(source) = source {
			json!({"op":"execute","id":self.id,"nonce":nonce,"source":source})
		} else {
			json!({"op":"shutdown","id":self.id,"nonce":nonce})
		};
		timeout(Duration::from_secs(5), self.send(request)).await??;
		let mut text: [Text; 2] = Default::default();
		let mut ended = [false; 2];
		let mut reply = None;
		let mut armed: Option<(u64, u64)> = None;
		loop {
			self.health()?;
			for index in 0..2 {
				loop {
					let (b, end, discarded, raw_end) = {
						let mut s = self.collected.lock().unwrap();
						let c = &mut s.active.as_mut().unwrap()[index];
						let b = c.take();
						let end = c.ended && b.is_empty();
						(b, end, c.discarded, c.retained_finished())
					};
					if b.is_empty() && !end {
						break;
					}
					if ended[index] {
						break;
					}
					let decoded = text[index].decode(&b, raw_end);
					if !decoded.is_empty() {
						sink(StreamEvent {
							name: ["stdout", "stderr"][index],
							text: decoded,
							replaced: text[index].replaced,
							unassociated: false,
						})
						.await?;
					}
					if end {
						ended[index] = true;
						if discarded > 0 {
							sink(StreamEvent {
								name: ["stdout", "stderr"][index],
								text: format!(
									"\n[rnx: {discarded} bytes discarded after the 2097152-byte stream limit]\n"
								),
								replaced: false,
								unassociated: false,
							})
							.await?;
						}
						break;
					}
				}
			}
			self.forward_late(sink).await?;
			if ended == [true, true] && reply.is_some() {
				break;
			}
			tokio::select! {
				_=self.notify.notified()=>{},
				message=self.control.recv()=>{
					let value=message.ok_or("worker control ended before settlement")?.value;
					if value["id"].as_u64()!=Some(self.id){return Err("worker reply identity mismatch".into());}
					match value["type"].as_str(){
						Some("armed") if armed.is_none() && reply.is_none()=>{
							let epoch=value["epoch"].as_u64().filter(|n|*n>0).ok_or("invalid armed epoch")?;
							let input=value["input"].as_u64().filter(|n|*n>0).ok_or("invalid armed input")?;
							armed=Some((epoch,input));self.interrupt.armed()?;
						},
						Some("settled") if reply.is_none()=>{
							let epoch=value["epoch"].as_u64().filter(|n|*n>0).ok_or("invalid settled epoch")?;
							if value.get("input").is_none()||value.get("text_plain").is_none()||value.get("failure").is_none()||!value["state_lost"].is_boolean(){return Err("incomplete worker settlement".into());}
							if !value["text_plain"].is_null()&&value["text_plain"].as_str().is_none_or(|s|s.len()>16384){return Err("invalid worker rendered result".into());}
							if value["input"].as_u64().map(|input|(epoch,input))!=armed{return Err("settled input differs from armed identity".into());}
							let failure=&value["failure"];
							if !failure.is_null() && (failure["diagnostic"].as_str().is_none_or(|s|s.len()>16384)||!matches!(failure["category"].as_str(),Some("compile"|"runtime"|"interrupted"|"refused"|"budget"|"over_ceiling"))){return Err("invalid worker failure".into());}
							reply=Some(value);
						},
						_=>return Err("unexpected worker control message".into()),
					}
				}
			}
		}
		complete(reply.as_ref().unwrap().clone()).await?;
		// Both barriers, the result and all retained output have reached the sink. A failed
		// send or cancellation never clears active ownership or fabricates ack.
		self.send(json!({"op":"ack","id":self.id})).await?;
		self.collected.lock().unwrap().active = None;
		self.interrupt.finish();
		Ok(reply.unwrap())
	}
	pub async fn forward_late<F, Fut>(&mut self, sink: &mut F) -> Res<()>
	where
		F: FnMut(StreamEvent) -> Fut,
		Fut: Future<Output = Res<()>>,
	{
		for index in 0..2 {
			loop {
				let (b, notice, raw_end) = {
					let mut s = self.collected.lock().unwrap();
					let l = &mut s.late[index];
					let end = (l.sent + 16384).min(l.data.len());
					let b = l.data[l.sent..end].to_vec();
					l.sent = end;
					let notice = l.discarded > 0 && !l.notified && end == l.data.len();
					if notice {
						l.notified = true;
					}
					(b, notice, end == CAP)
				};
				if b.is_empty() && !notice {
					break;
				}
				let text = self.late_text[index].decode(&b, raw_end);
				if !text.is_empty() {
					sink(StreamEvent {
						name: ["stdout", "stderr"][index],
						text,
						replaced: self.late_text[index].replaced,
						unassociated: true,
					})
					.await?;
				}
				if notice {
					sink(StreamEvent {
						name: ["stdout", "stderr"][index],
						text:
							"\n[rnx: unassociated stream limit reached; further bytes discarded]\n"
								.into(),
						replaced: false,
						unassociated: true,
					})
					.await?;
				}
			}
		}
		Ok(())
	}
	pub async fn terminate(&mut self, end: Instant) -> Res<()> {
		self.interrupt.disable();
		self.child.kill = false;
		let result = containment::sweep(self.child.child.id() as i32, end).await;
		self.tasks.abort_all();
		while self.tasks.join_next().await.is_some() {}
		result
	}
}
impl Drop for Worker {
	fn drop(&mut self) {
		self.interrupt.disable();
		self.tasks.abort_all();
	}
}
