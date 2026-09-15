//! Owned server-side ZMTP transport, extracted from the accepted 0048 probe.
//! Application callbacks borrow the complete message: no inbound message queue,
//! and no transfer of receive credits to an unaccounted task or channel.
use serde_json::{Value, json};
use std::{
	collections::HashMap,
	sync::{
		Arc, Mutex,
		atomic::{AtomicU64, AtomicUsize, Ordering::SeqCst},
	},
	time::Duration,
};
use tokio::{
	io::{AsyncReadExt, AsyncWriteExt},
	net::{TcpListener, TcpStream},
	sync::{OwnedSemaphorePermit, Semaphore, mpsc, watch},
	time::{Instant, timeout, timeout_at},
};
const M: usize = 1024 * 1024;
pub type Res<T> = Result<T, Box<dyn std::error::Error + Send + Sync>>;
fn err(s: &str) -> Box<dyn std::error::Error + Send + Sync> {
	s.into()
}
#[derive(Default)]
struct Stats {
	last_failure: Mutex<String>,
	received: AtomicUsize,
	rejected: AtomicUsize,
	sent: AtomicUsize,
	closed: AtomicUsize,
	generated_collisions: AtomicUsize,
}
struct Packet {
	wire: Arc<Vec<u8>>,
	_bytes: OwnedSemaphorePermit,
	_count: OwnedSemaphorePermit,
	_global: Option<OwnedSemaphorePermit>,
}
struct Peer {
	id: Vec<u8>,
	generation: u64,
	tx: mpsc::Sender<Packet>,
	stop: watch::Sender<bool>,
	bytes: Arc<Semaphore>,
	count: Arc<Semaphore>,
	subs: Mutex<HashMap<Vec<u8>, u16>>,
	writing: AtomicUsize,
}
struct Endpoint {
	name: &'static str,
	kind: &'static str,
	peers: Mutex<HashMap<Vec<u8>, Arc<Peer>>>,
	slots: Arc<Semaphore>,
	incoming: Arc<Semaphore>,
}
struct State {
	#[cfg(feature = "transport-probe")]
	trace: Mutex<Vec<Value>>,
	stats: Stats,
	next: AtomicU64,
	publication: Arc<Semaphore>,
	endpoints: Vec<Arc<Endpoint>>,
}
struct Message {
	parts: Vec<Vec<u8>>,
	_credits: Vec<OwnedSemaphorePermit>,
}
fn encode(parts: &[Vec<u8>]) -> Res<Arc<Vec<u8>>> {
	let size = parts
		.iter()
		.try_fold(0usize, |n, p| n.checked_add(p.len())?.checked_add(9))
		.ok_or("wire overflow")?;
	if size > 2 * M {
		return Err(err("outgoing wire cap"));
	}
	let mut b = Vec::with_capacity(size);
	for (i, p) in parts.iter().enumerate() {
		let more = if i + 1 < parts.len() { 1 } else { 0 };
		if p.len() < 256 {
			b.extend([more, p.len() as u8]);
		} else {
			b.push(more | 2);
			b.extend((p.len() as u64).to_be_bytes());
		}
		b.extend(p);
	}
	Ok(Arc::new(b))
}
// Parse the u64 before converting to usize or allocating. A fixed nine-byte header.
async fn header<R: tokio::io::AsyncRead + Unpin>(r: &mut R) -> Res<(u8, u64)> {
	let flag = r.read_u8().await?;
	if flag & 0xf8 != 0 || flag & 5 == 5 {
		return Err(err("invalid flags"));
	}
	let n = if flag & 2 != 0 {
		r.read_u64().await?
	} else {
		r.read_u8().await? as u64
	};
	Ok((flag, n))
}
async fn body<R: tokio::io::AsyncRead + Unpin>(r: &mut R, n: u64, cap: usize) -> Res<Vec<u8>> {
	if n > cap as u64 {
		return Err(err("declared length exceeds cap"));
	}
	let n = usize::try_from(n)?;
	let mut b = Vec::new();
	b.try_reserve_exact(n)?;
	b.resize(n, 0);
	r.read_exact(&mut b).await?;
	Ok(b)
}
#[cfg(test)]
async fn message<R: tokio::io::AsyncRead + Unpin>(r: &mut R, ep: &Endpoint) -> Res<Message> {
	message_with(r, ep, None).await
}
fn heartbeat(command: &[u8], peer: Option<&Peer>) -> Res<()> {
	if command.starts_with(b"\x04PING") && (7..=23).contains(&command.len()) {
		let mut wire = Vec::with_capacity(command.len());
		wire.extend([4, (command.len() - 2) as u8]);
		wire.extend(b"\x04PONG");
		wire.extend(&command[7..]);
		enqueue(peer.ok_or("heartbeat without peer")?, Arc::new(wire), None)
	} else if command.starts_with(b"\x04PONG") && (5..=21).contains(&command.len()) {
		Ok(())
	} else {
		Err(err("malformed or unsupported traffic command"))
	}
}
async fn message_with<R: tokio::io::AsyncRead + Unpin>(
	r: &mut R,
	ep: &Endpoint,
	peer: Option<&Peer>,
) -> Res<Message> {
	// Idle connections have no assembly deadline. Start after the first header byte.
	let first = r.read_u8().await?;
	let end = Instant::now() + Duration::from_secs(5);
	timeout_at(end, async {
		let mut parts = Vec::with_capacity(32);
		let mut credits = Vec::with_capacity(32);
		let mut total = 0usize;
		let mut first = Some(first);
		loop {
			let (f, n) = if let Some(f) = first.take() {
				if f & 0xf8 != 0 || f & 5 == 5 {
					return Err(err("invalid flags"));
				}
				let n = if f & 2 != 0 {
					r.read_u64().await?
				} else {
					r.read_u8().await? as u64
				};
				(f, n)
			} else {
				header(r).await?
			};
			if f & 4 != 0 {
				let command = body(r, n, 23).await?;
				heartbeat(&command, peer)?;
				tokio::task::yield_now().await;
				if parts.is_empty() {
					return Ok(Message {
						parts,
						_credits: credits,
					});
				}
				continue;
			}

			if parts.len() == 32 || n > (M - total) as u64 {
				return Err(err("multipart cap"));
			}
			let len = usize::try_from(n)?;
			let credit = ep.incoming.clone().acquire_many_owned(len as u32).await?;
			let b = body(r, n, M - total).await?;
			if b.capacity() != len {
				return Err(err("unexpected allocation capacity"));
			}
			credits.push(credit);
			total += len;
			parts.push(b);
			if f & 1 == 0 {
				return Ok(Message {
					parts,
					_credits: credits,
				});
			}
		}
	})
	.await?
}
fn metadata(kind: &str) -> Vec<u8> {
	let mut b = b"\x05READY\x0bSocket-Type".to_vec();
	b.extend((kind.len() as u32).to_be_bytes());
	b.extend(kind.as_bytes());
	b
}
async fn handshake(s: &mut TcpStream, ep: &Endpoint, st: &State) -> Res<Option<Vec<u8>>> {
	timeout(Duration::from_secs(2), async {
		let mut greeting = [0u8; 64];
		greeting[0] = 255;
		greeting[9] = 127;
		greeting[10] = 3;
		greeting[12..16].copy_from_slice(b"NULL");
		s.write_all(&greeting).await?;
		s.read_exact(&mut greeting).await?;
		trace(
			st,
			json!({"endpoint":ep.name,"peer_greeting":hex::encode(greeting)}),
		);
		if greeting[0] != 255
			|| greeting[9] != 127
			|| greeting[10] < 3
			|| greeting[12..32] != *b"NULL\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0"
			|| greeting[32] != 0
		{
			return Err(err("greeting"));
		}
		let ready = metadata(ep.kind);
		trace(
			st,
			json!({"endpoint":ep.name,"server_version":[3,0],"server_ready":hex::encode(&ready)}),
		);
		s.write_all(&[4, ready.len() as u8]).await?;
		s.write_all(&ready).await?;
		let (f, n) = header(s).await?;
		if f & 4 == 0 {
			return Err(err("READY command required"));
		}
		let ready = body(s, n, 8192).await?;
		trace(
			st,
			json!({"endpoint":ep.name,"peer_ready_prefix":hex::encode(&ready[..ready.len().min(512)])}),
		);
		if !ready.starts_with(b"\x05READY") {
			return Err(err("READY required"));
		}
		let mut props = HashMap::new();
		let mut p = 6;
		while p < ready.len() {
			if props.len() == 64 {
				return Err(err("metadata count"));
			}
			let n = ready[p] as usize;
			p += 1;
			if n == 0 || p + n + 4 > ready.len() {
				return Err(err("metadata name"));
			}
			let name = ready[p..p + n]
				.iter()
				.map(u8::to_ascii_lowercase)
				.collect::<Vec<_>>();
			if !name
				.iter()
				.all(|c| c.is_ascii_alphanumeric() || b"-_.+".contains(c))
			{
				return Err(err("metadata name"));
			}
			p += n;
			let len = u32::from_be_bytes(ready[p..p + 4].try_into()?) as usize;
			p += 4;
			if len > ready.len() - p {
				return Err(err("metadata length"));
			}
			if props.insert(name, ready[p..p + len].to_vec()).is_some() {
				return Err(err("duplicate metadata"));
			}
			p += len;
		}
		let want = match ep.kind {
			"PUB" => b"SUB".as_slice(),
			"REP" => b"REQ".as_slice(),
			_ => b"DEALER".as_slice(),
		};
		if props.get(b"socket-type".as_slice()).map(Vec::as_slice) != Some(want) {
			return Err(err("socket pairing"));
		}
		let id = props.remove(b"identity".as_slice());
		if id.as_ref().is_some_and(|id| id.len() > 255) {
			return Err(err("identity length"));
		}
		Ok(id.filter(|i| !i.is_empty()))
	})
	.await?
}
fn enqueue(peer: &Peer, wire: Arc<Vec<u8>>, global: Option<OwnedSemaphorePermit>) -> Res<()> {
	if *peer.stop.borrow() {
		return Err(err("closed generation"));
	}
	let bytes = peer
		.bytes
		.clone()
		.try_acquire_many_owned(wire.capacity() as u32)?;
	let count = peer.count.clone().try_acquire_owned()?;
	peer.tx
		.try_send(Packet {
			wire,
			_bytes: bytes,
			_count: count,
			_global: global,
		})
		.map_err(|_| err("reply admission"))?;
	Ok(())
}
fn publish(st: &State, parts: &[Vec<u8>]) -> Res<()> {
	let wire = encode(parts)?;
	let ep = &st.endpoints[4];
	let peers = ep.peers.lock().unwrap();
	let mut reserved = vec![];
	for peer in peers.values() {
		if *peer.stop.borrow() {
			continue;
		}
		if !peer
			.subs
			.lock()
			.unwrap()
			.keys()
			.any(|p| parts[0].starts_with(p))
		{
			continue;
		}
		let bytes = peer
			.bytes
			.clone()
			.try_acquire_many_owned(wire.capacity() as u32);
		let count = peer.count.clone().try_acquire_owned();
		if let (Ok(bytes), Ok(count)) = (bytes, count) {
			reserved.push((peer, bytes, count));
		} else {
			peer.stop.send_replace(true);
		}
	}
	// Reserve the whole fanout before any publication enters a writer queue.
	let mut globals = vec![];
	for _ in &reserved {
		globals.push(
			st.publication
				.clone()
				.try_acquire_many_owned(wire.capacity() as u32)?,
		);
	}
	for ((peer, bytes, count), global) in reserved.into_iter().zip(globals) {
		let _ = peer.tx.try_send(Packet {
			wire: wire.clone(),
			_bytes: bytes,
			_count: count,
			_global: Some(global),
		});
	}
	st.stats.sent.fetch_add(1, SeqCst);
	Ok(())
}
fn subscription(peer: &Peer, m: &Message) -> Res<()> {
	if m.parts.len() != 1 || m.parts[0].is_empty() || m.parts[0].len() > 257 {
		return Err(err("subscription"));
	}
	let command = m.parts[0][0];
	let key = &m.parts[0][1..];
	let mut subs = peer.subs.lock().unwrap();
	match command {
		1 => {
			if !subs.contains_key(key) && subs.len() == 128 {
				return Err(err("subscription count"));
			}
			let v = subs.entry(key.to_vec()).or_insert(0);
			*v = v.checked_add(1).ok_or("subscription overflow")?;
		}
		0 => {
			if let Some(n) = subs.get_mut(key) {
				*n -= 1;
				if *n == 0 {
					subs.remove(key);
				}
			}
		}
		_ => return Err(err("subscription prefix")),
	};
	Ok(())
}
struct Writing<'a>(&'a AtomicUsize);
impl Drop for Writing<'_> {
	fn drop(&mut self) {
		self.0.store(0, SeqCst);
	}
}
async fn write_packets<W: tokio::io::AsyncWrite + Unpin>(
	write: &mut W,
	rx: &mut mpsc::Receiver<Packet>,
	peer: &Peer,
) -> Res<()> {
	while let Some(p) = rx.recv().await {
		peer.writing.store(p.wire.len(), SeqCst);
		let _writing = Writing(&peer.writing);
		timeout(Duration::from_secs(5), write.write_all(&p.wire)).await??;
	}
	Ok(())
}
// The application is now a separate component. Its future can unwind or be
// cancelled, so retirement must not depend on reaching the end of connection().
struct Connected<'a> {
	peer: &'a Arc<Peer>,
	endpoint: &'a Endpoint,
	stats: &'a Stats,
}
impl Drop for Connected<'_> {
	fn drop(&mut self) {
		self.peer.stop.send_replace(true);
		let mut peers = self.endpoint.peers.lock().unwrap();
		if peers
			.get(&self.peer.id)
			.is_some_and(|p| p.generation == self.peer.generation)
		{
			peers.remove(&self.peer.id);
		}
		self.stats.closed.fetch_add(1, SeqCst);
	}
}
async fn connection(
	mut s: TcpStream,
	ep: Arc<Endpoint>,
	st: Arc<State>,
	handler: Arc<dyn Handler>,
	mut shutdown: watch::Receiver<bool>,
) -> Res<()> {
	if *shutdown.borrow() {
		return Ok(());
	}
	s.set_nodelay(true)?;
	let id = tokio::select! {r=handshake(&mut s,&ep,&st)=>r?,_=shutdown.changed()=>return Ok(())};
	if *shutdown.borrow() {
		return Ok(());
	}
	let (tx, mut rx) = mpsc::channel(64);
	let (stop, mut stopped) = watch::channel(false);
	let peer = {
		let mut peers = ep.peers.lock().unwrap();
		let mut generation = st
			.next
			.fetch_update(SeqCst, SeqCst, |v| v.checked_add(1))
			.map_err(|_| err("generation exhausted"))?;
		let id = if let Some(id) = id {
			if peers.contains_key(&id) {
				return Err(err("live duplicate identity"));
			}
			id
		} else {
			loop {
				let mut id = vec![0];
				id.extend(generation.to_be_bytes());
				if !peers.contains_key(&id) {
					break id;
				}
				st.stats.generated_collisions.fetch_add(1, SeqCst);
				generation = st
					.next
					.fetch_update(SeqCst, SeqCst, |v| v.checked_add(1))
					.map_err(|_| err("generation exhausted"))?;
			}
		};
		let peer = Arc::new(Peer {
			id: id.clone(),
			generation,
			tx,
			stop,
			bytes: Arc::new(Semaphore::new(if ep.kind == "PUB" { 2 * M } else { M })),
			count: Arc::new(Semaphore::new(if ep.kind == "PUB" { 64 } else { 32 })),
			subs: Mutex::new(HashMap::new()),
			writing: AtomicUsize::new(0),
		});
		peers.insert(id, peer.clone());
		peer
	};
	let _connected = Connected {
		peer: &peer,
		endpoint: &ep,
		stats: &st.stats,
	};
	let (mut read, mut write) = s.into_split();
	let read_loop = async {
		loop {
			let m = message_with(&mut read, &ep, Some(&peer)).await?;
			if m.parts.is_empty() {
				continue;
			}
			st.stats.received.fetch_add(1, SeqCst);
			match ep.kind {
				"PUB" => {
					trace(
						&st,
						json!({"subscription_parts":m.parts.len(),"first_part_prefix":hex::encode(&m.parts[0][..m.parts[0].len().min(257)])}),
					);
					subscription(&peer, &m)?
				}
				"REP" => {
					if m.parts.first().is_none_or(|p| !p.is_empty()) {
						return Err(err("REQ envelope"));
					}
					enqueue(&peer, encode(&m.parts)?, None)?;
				}
				_ => {
					handler
						.handle(ep.name, Route(peer.clone()), &m.parts)
						.await?
				}
			};
			tokio::task::yield_now().await;
		}
		#[allow(unreachable_code)]
		Ok::<(), Box<dyn std::error::Error + Send + Sync>>(())
	};
	let write_loop = write_packets(&mut write, &mut rx, &peer);

	let result = tokio::select! {r=read_loop=>r,r=write_loop=>r,_=stopped.changed()=>Ok(()),_=shutdown.changed()=>Ok(())};
	drop(rx);
	result
}
#[cfg(feature = "transport-probe")]
fn trace(st: &State, v: Value) {
	let mut t = st.trace.lock().unwrap();
	if t.len() < 64 {
		t.push(v);
	}
}
#[cfg(not(feature = "transport-probe"))]
fn trace(_: &State, _: Value) {}

#[cfg(feature = "transport-probe")]
fn snapshot(st: &State) -> Value {
	json!({"last_failure":st.stats.last_failure.lock().unwrap().clone(),"wire_trace":st.trace.lock().unwrap().clone(),"next_generation":st.next.load(SeqCst),"rejected":st.stats.rejected.load(SeqCst),"messages_received":st.stats.received.load(SeqCst),"pub_send_ok":st.stats.sent.load(SeqCst),"generated_collisions":st.stats.generated_collisions.load(SeqCst),"publication_reserved":16*M-st.publication.available_permits(),"endpoints":st.endpoints.iter().map(|e|{let peers=e.peers.lock().unwrap();json!({"name":e.name,"connections":8-e.slots.available_permits(),"incoming_reserved":8*M-e.incoming.available_permits(),"routes":peers.len(),"identities":peers.values().map(|p|json!({"id":hex::encode(&p.id),"generation":p.generation,"writing":p.writing.load(SeqCst)})).collect::<Vec<_>>(),"outgoing_reserved":peers.values().map(|p|(if e.kind=="PUB"{2*M}else{M})-p.bytes.available_permits()).sum::<usize>(),"subscriptions":peers.values().map(|p|p.subs.lock().unwrap().len()).sum::<usize>()})}).collect::<Vec<_>>()})
}

/// An application handles one message while the transport retains its credits.
/// Any retained application representation needs separate admission before copy.
pub trait Handler: Send + Sync + 'static {
	fn handle<'a>(
		&'a self,
		channel: &'static str,
		route: Route,
		parts: &'a [Vec<u8>],
	) -> std::pin::Pin<Box<dyn std::future::Future<Output = Res<()>> + Send + 'a>>;
}

/// The original connection generation, never an identity lookup after reconnect.
#[derive(Clone)]
pub struct Route(Arc<Peer>);
impl Route {
	pub fn reply(&self, parts: &[Vec<u8>]) -> Res<()> {
		let result = encode(parts).and_then(|wire| enqueue(&self.0, wire, None));
		if result.is_err() {
			self.0.stop.send_replace(true);
		}
		result
	}
	pub fn is_closed(&self) -> bool {
		*self.0.stop.borrow()
	}
}

/// Cloneable publication capability; use a single application publishing owner.
#[derive(Clone)]
pub struct Publisher {
	state: Arc<State>,
	shutdown: watch::Receiver<bool>,
}
impl Publisher {
	pub fn publish(&self, parts: &[Vec<u8>]) -> Res<()> {
		if parts.is_empty() {
			return Err(err("publication needs a topic"));
		}
		if *self.shutdown.borrow() {
			return Err(err("transport is shutting down"));
		}
		publish(&self.state, parts)
	}
}

/// Owns all listeners and connection tasks. Dropping it signals cancellation;
/// call shutdown to await collection and report a failure of the five-second bound.
pub struct Server {
	state: Arc<State>,
	stop: watch::Sender<bool>,
	tasks: tokio::task::JoinSet<()>,
	addresses: Vec<std::net::SocketAddr>,
}
impl Server {
	/// Endpoint order: shell, control, stdin, heartbeat, IOPub.
	/// Bind all listeners before spawning anything: partial startup leaks no tasks.
	pub async fn bind(
		addresses: [std::net::SocketAddr; 5],
		handler: Arc<dyn Handler>,
	) -> Res<Self> {
		let mut listeners = Vec::new();
		for address in addresses {
			if !address.ip().is_loopback() {
				return Err(err("transport requires loopback"));
			}
			listeners.push(TcpListener::bind(address).await?);
		}
		let addresses = listeners
			.iter()
			.map(TcpListener::local_addr)
			.collect::<Result<Vec<_>, _>>()?;
		let endpoints = [
			("shell", "ROUTER"),
			("control", "ROUTER"),
			("stdin", "ROUTER"),
			("hb", "REP"),
			("iopub", "PUB"),
		]
		.into_iter()
		.map(|(name, kind)| {
			Arc::new(Endpoint {
				name,
				kind,
				peers: Mutex::new(HashMap::new()),
				slots: Arc::new(Semaphore::new(8)),
				incoming: Arc::new(Semaphore::new(8 * M)),
			})
		})
		.collect();
		let state = Arc::new(State {
			#[cfg(feature = "transport-probe")]
			trace: Mutex::new(vec![]),
			stats: Stats::default(),
			next: AtomicU64::new(1),
			publication: Arc::new(Semaphore::new(16 * M)),
			endpoints,
		});
		let (stop, shutdown) = watch::channel(false);
		let mut tasks = tokio::task::JoinSet::new();
		for (ep, listener) in state.endpoints.iter().cloned().zip(listeners) {
			let st = state.clone();
			let handler = handler.clone();
			let mut shutdown = shutdown.clone();
			tasks.spawn(async move {
                let mut children=tokio::task::JoinSet::new();
                // Admission includes completed task records until joined.
                let mut permits=HashMap::new();
                loop {
                    tokio::select! {
                        _=shutdown.changed()=>break,
                        result=children.join_next_with_id(), if !children.is_empty()=>{
                            let id=match result.unwrap(){Ok((id,_))=>id,Err(e)=>e.id()};permits.remove(&id);
                        },
                        accepted=listener.accept()=>{
                            let Ok((s,_))=accepted else{break;};
                            if let Ok(slot)=ep.slots.clone().try_acquire_owned(){
                                let ep=ep.clone();let st=st.clone();let handler=handler.clone();let shutdown=shutdown.clone();
                                let task=children.spawn(async move {
                                    if let Err(e)=connection(s,ep,st.clone(),handler,shutdown).await {
                                        *st.stats.last_failure.lock().unwrap()=e.to_string().chars().take(512).collect();st.stats.rejected.fetch_add(1,SeqCst);
                                    }
                                });
                                permits.insert(task.id(),slot);
                            } else {drop(s);st.stats.rejected.fetch_add(1,SeqCst);}
                        }
                    }
                }
                while let Some(result)=children.join_next_with_id().await {
                    let id=match result{Ok((id,_))=>id,Err(e)=>e.id()};permits.remove(&id);
                }
            });
		}
		Ok(Self {
			state,
			stop,
			tasks,
			addresses,
		})
	}
	pub fn addresses(&self) -> &[std::net::SocketAddr] {
		&self.addresses
	}
	pub fn publisher(&self) -> Publisher {
		Publisher {
			state: self.state.clone(),
			shutdown: self.stop.subscribe(),
		}
	}
	#[cfg(feature = "transport-probe")]
	pub fn snapshot(&self) -> Value {
		snapshot(&self.state)
	}
	pub async fn shutdown(&mut self) -> Res<()> {
		self.stop.send_replace(true);
		timeout(Duration::from_secs(5), async {
			while let Some(result) = self.tasks.join_next().await {
				result?;
			}
			Ok::<_, Box<dyn std::error::Error + Send + Sync>>(())
		})
		.await??;
		Ok(())
	}
}
impl Drop for Server {
	fn drop(&mut self) {
		self.stop.send_replace(true);
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	fn ep() -> Endpoint {
		Endpoint {
			name: "test",
			kind: "ROUTER",
			peers: Mutex::new(HashMap::new()),
			slots: Arc::new(Semaphore::new(8)),
			incoming: Arc::new(Semaphore::new(8 * M)),
		}
	}
	#[tokio::test]
	async fn split_headers_and_coalesced_frames() {
		for n in [0, 1, 255, 256, 4096] {
			let parts = vec![vec![42; n], vec![], b"tail".to_vec()];
			let wire = encode(&parts).unwrap();
			for split in 0..=9.min(wire.len()) {
				let (mut tx, mut rx) = tokio::io::duplex(16384);
				let w = wire.clone();
				tokio::spawn(async move {
					tx.write_all(&w[..split]).await.unwrap();
					tokio::task::yield_now().await;
					tx.write_all(&w[split..]).await.unwrap();
				});
				let e = ep();
				let m = message(&mut rx, &e).await.unwrap();
				assert_eq!(m.parts, parts);
				drop(m);
				assert_eq!(e.incoming.available_permits(), 8 * M);
			}
		}
	}
	#[tokio::test]
	async fn caps_are_checked_before_body_or_narrowing() {
		for n in [M as u64 + 1, u32::MAX as u64, u64::MAX - 1, u64::MAX] {
			let mut wire = vec![2];
			wire.extend(n.to_be_bytes());
			let e = ep();
			assert!(message(&mut wire.as_slice(), &e).await.is_err());
			assert_eq!(e.incoming.available_permits(), 8 * M);
		}
		let e = ep();
		let mut wire = vec![0, 0];
		for _ in 0..32 {
			wire.splice(0..0, [1, 0]);
		}
		assert!(message(&mut wire.as_slice(), &e).await.is_err());
		let parts = vec![vec![]; 32];
		let encoded = encode(&parts).unwrap();
		assert_eq!(
			message(&mut encoded.as_slice(), &e)
				.await
				.unwrap()
				.parts
				.len(),
			32
		);
		let encoded = encode(&[vec![7; M]]).unwrap();
		let m = message(&mut encoded.as_slice(), &e).await.unwrap();
		assert_eq!(e.incoming.available_permits(), 7 * M);
		drop(m);
		assert_eq!(e.incoming.available_permits(), 8 * M);
	}
	#[tokio::test]
	async fn malformed_and_truncated_states_release_credits() {
		for f in [5, 7, 8, 128, 255] {
			let e = ep();
			assert!(message(&mut [f, 0].as_slice(), &e).await.is_err());
			assert_eq!(e.incoming.available_permits(), 8 * M);
		}
		let wire = encode(&[b"first".to_vec(), vec![3; 256]]).unwrap();
		for n in 0..wire.len() {
			let e = ep();
			assert!(message(&mut wire[..n].as_ref(), &e).await.is_err());
			assert_eq!(e.incoming.available_permits(), 8 * M);
		}
	}
	fn peer(limit: usize) -> (Arc<Peer>, mpsc::Receiver<Packet>) {
		let (tx, rx) = mpsc::channel(64);
		let (stop, _) = watch::channel(false);
		(
			Arc::new(Peer {
				id: vec![0],
				generation: 1,
				tx,
				stop,
				bytes: Arc::new(Semaphore::new(limit)),
				count: Arc::new(Semaphore::new(64)),
				subs: Mutex::new(HashMap::new()),
				writing: AtomicUsize::new(0),
			}),
			rx,
		)
	}
	#[test]
	fn subscription_and_reply_credit_boundaries() {
		let (p, mut rx) = peer(2 * M);
		for n in 0..128 {
			let key = vec![1, n];
			subscription(
				&p,
				&Message {
					parts: vec![key],
					_credits: vec![],
				},
			)
			.unwrap();
		}
		assert!(
			subscription(
				&p,
				&Message {
					parts: vec![vec![1, 128]],
					_credits: vec![]
				}
			)
			.is_err()
		);
		for _ in 1..65535 {
			subscription(
				&p,
				&Message {
					parts: vec![vec![1, 0]],
					_credits: vec![],
				},
			)
			.unwrap();
		}
		assert!(
			subscription(
				&p,
				&Message {
					parts: vec![vec![1, 0]],
					_credits: vec![]
				}
			)
			.is_err()
		);
		let (p2, _rx2) = peer(M);
		subscription(
			&p2,
			&Message {
				parts: vec![vec![1; 257]],
				_credits: vec![],
			},
		)
		.unwrap();
		assert!(
			subscription(
				&p2,
				&Message {
					parts: vec![vec![1; 258]],
					_credits: vec![]
				}
			)
			.is_err()
		);
		let wire = Arc::new(vec![0; 2 * M]);
		enqueue(&p, wire.clone(), None).unwrap();
		assert!(enqueue(&p, Arc::new(vec![1]), None).is_err());
		assert_eq!(p.bytes.available_permits(), 0);
		drop(rx.try_recv().unwrap());
		assert_eq!(p.bytes.available_permits(), 2 * M);
		p.stop.send_replace(true);
		assert!(enqueue(&p, wire, None).is_err());
	}
	#[test]
	fn publication_preflight_and_closing_stale_generation() {
		let e = Arc::new(ep());
		let mut endpoints = vec![e.clone(); 5];
		endpoints[4] = e.clone();
		let st = State {
			#[cfg(feature = "transport-probe")]
			trace: Mutex::new(vec![]),
			stats: Stats::default(),
			next: AtomicU64::new(1),
			publication: Arc::new(Semaphore::new(16 * M)),
			endpoints,
		};
		let (p, mut rx) = peer(2 * M);
		p.subs.lock().unwrap().insert(vec![], 1);
		e.peers.lock().unwrap().insert(vec![0], p.clone());
		let all = st
			.publication
			.clone()
			.try_acquire_many_owned((16 * M) as u32)
			.unwrap();
		assert!(publish(&st, &[b"abc".to_vec()]).is_err());
		assert!(rx.try_recv().is_err());
		assert_eq!(p.bytes.available_permits(), 2 * M);
		drop(all);
		publish(&st, &[b"abc".to_vec()]).unwrap();
		let packet = rx.try_recv().unwrap();
		assert!(st.publication.available_permits() < 16 * M);
		drop(packet);
		assert_eq!(st.publication.available_permits(), 16 * M);
		p.stop.send_replace(true);
		let (new, _) = peer(2 * M);
		e.peers.lock().unwrap().insert(vec![0], new.clone());
		assert!(enqueue(&p, encode(&[b"stale".to_vec()]).unwrap(), None).is_err());
		assert_eq!(new.bytes.available_permits(), 2 * M);
	}
	#[tokio::test]
	async fn heartbeat_context_bounds_and_interleaving() {
		let (p, mut rx) = peer(2 * M);
		for n in [0, 16] {
			let mut ping = b"\x04PING\xff\xff".to_vec();
			ping.extend(vec![42; n]);
			heartbeat(&ping, Some(&p)).unwrap();
			let pong = rx.try_recv().unwrap();
			assert_eq!(&pong.wire[2..7], b"\x04PONG");
			assert_eq!(&pong.wire[7..], vec![42; n]);
			heartbeat(&pong.wire[2..], Some(&p)).unwrap();
			assert!(rx.try_recv().is_err());
		}
		for b in [
			b"\x04PING\0".to_vec(),
			[b"\x04PING\0\0".as_slice(), &[42; 17]].concat(),
			[b"\x04PONG".as_slice(), &[42; 17]].concat(),
			b"\x03PING\0\0".to_vec(),
		] {
			assert!(heartbeat(&b, Some(&p)).is_err());
		}
		let mut wire = vec![1, 1, b'a', 4, 7];
		wire.extend(b"\x04PING\0\0");
		wire.extend([0, 1, b'b']);
		let e = ep();
		let m = message_with(&mut wire.as_slice(), &e, Some(&p))
			.await
			.unwrap();
		assert_eq!(m.parts, vec![b"a".to_vec(), b"b".to_vec()]);
		drop(m);
		drop(rx.try_recv().unwrap());
		assert_eq!(e.incoming.available_permits(), 8 * M);
		for _ in 0..64 {
			heartbeat(b"\x04PING\0\0", Some(&p)).unwrap();
		}
		assert!(heartbeat(b"\x04PING\0\0", Some(&p)).is_err());
		drop(rx);
		assert_eq!(p.bytes.available_permits(), 2 * M);
		assert_eq!(p.count.available_permits(), 64);
	}
	#[test]
	fn all_publication_credit_and_entry_boundaries() {
		let e = Arc::new(ep());
		let st = State {
			#[cfg(feature = "transport-probe")]
			trace: Mutex::new(vec![]),
			stats: Stats::default(),
			next: AtomicU64::new(1),
			publication: Arc::new(Semaphore::new(16 * M)),
			endpoints: vec![e.clone(); 5],
		};
		let mut receivers = vec![];
		// The encoder reserves nine framing bytes per part, charged as capacity.
		let body = vec![7; M - 9];
		for id in 0..8 {
			let (p, rx) = peer(2 * M);
			p.subs.lock().unwrap().insert(vec![], 1);
			e.peers.lock().unwrap().insert(vec![id], p);
			receivers.push(rx);
		}
		publish(&st, &[body.clone()]).unwrap();
		publish(&st, &[body]).unwrap();
		assert_eq!(st.publication.available_permits(), 0);
		// A byte beyond a subscriber's allowance retires it without growing queues.
		publish(&st, &[vec![1]]).unwrap();
		assert!(e.peers.lock().unwrap().values().all(|p| *p.stop.borrow()));
		drop(receivers);
		assert_eq!(st.publication.available_permits(), 16 * M);
		let (p, rx) = peer(M);
		for _ in 0..64 {
			enqueue(&p, Arc::new(vec![]), None).unwrap();
		}
		assert!(enqueue(&p, Arc::new(vec![]), None).is_err());
		drop(rx);
		assert_eq!(p.count.available_permits(), 64);
		let budget = Arc::new(Semaphore::new(8 * M));
		let mut credits = vec![];
		for _ in 0..8 {
			credits.push(budget.clone().try_acquire_many_owned(M as u32).unwrap());
		}
		assert!(budget.clone().try_acquire_owned().is_err());
		drop(credits);
		assert_eq!(budget.available_permits(), 8 * M);
	}

	#[tokio::test]
	async fn partial_writer_cancellation_releases_current_and_queued() {
		let (p, mut rx) = peer(2 * M);
		enqueue(&p, Arc::new(vec![1; 1024]), None).unwrap();
		enqueue(&p, Arc::new(vec![2; 1024]), None).unwrap();
		let (mut writer, mut reader) = tokio::io::duplex(8);
		{
			let future = write_packets(&mut writer, &mut rx, &p);
			tokio::pin!(future);
			tokio::select! {r=&mut future=>panic!("writer unexpectedly completed: {r:?}"),_=tokio::time::sleep(Duration::from_millis(20))=>{}}
			assert_eq!(p.writing.load(SeqCst), 1024);
			let mut prefix = [0; 8];
			reader.read_exact(&mut prefix).await.unwrap();
			assert_eq!(prefix, [1; 8]);
			assert_eq!(p.bytes.available_permits(), 2 * M - 2048);
		}
		assert_eq!(p.writing.load(SeqCst), 0);
		assert_eq!(p.bytes.available_permits(), 2 * M - 1024);
		drop(rx);
		assert_eq!(p.bytes.available_permits(), 2 * M);
	}
	#[test]
	fn thirty_two_reply_slots_include_current_write() {
		let (mut p, rx) = peer(M);
		Arc::get_mut(&mut p).unwrap().count = Arc::new(Semaphore::new(32));
		for _ in 0..32 {
			enqueue(&p, Arc::new(vec![1]), None).unwrap();
		}
		assert!(enqueue(&p, Arc::new(vec![1]), None).is_err());
		drop(rx);
		assert_eq!(p.count.available_permits(), 32);
	}

	#[test]
	fn aggregate_failure_never_partially_publishes() {
		let e = Arc::new(ep());
		let st = State {
			#[cfg(feature = "transport-probe")]
			trace: Mutex::new(vec![]),
			stats: Stats::default(),
			next: AtomicU64::new(1),
			publication: Arc::new(Semaphore::new(16 * M)),
			endpoints: vec![e.clone(); 5],
		};
		let mut receivers = vec![];
		for id in 0..8 {
			let (p, rx) = peer(2 * M);
			p.subs.lock().unwrap().insert(vec![], 1);
			e.peers.lock().unwrap().insert(vec![id], p);
			receivers.push(rx);
		}
		let held = st
			.publication
			.clone()
			.try_acquire_many_owned((9 * M) as u32)
			.unwrap();
		assert!(publish(&st, &[vec![1; M - 9]]).is_err());
		assert!(receivers.iter_mut().all(|r| r.try_recv().is_err()));
		assert!(
			e.peers
				.lock()
				.unwrap()
				.values()
				.all(|p| p.bytes.available_permits() == 2 * M)
		);
		assert_eq!(st.publication.available_permits(), 7 * M);
		drop(held);
		assert_eq!(st.publication.available_permits(), 16 * M);
		let (p, mut rx) = peer(M);
		enqueue(&p, Arc::new(vec![1; M]), None).unwrap();
		assert!(enqueue(&p, Arc::new(vec![1]), None).is_err());
		drop(rx.try_recv().unwrap());
		assert_eq!(p.bytes.available_permits(), M);
	}
	#[tokio::test]
	async fn writing_deadline_does_not_restart_on_progress() {
		let (p, mut rx) = peer(M);
		enqueue(&p, Arc::new(vec![1; 1024]), None).unwrap();
		let (mut writer, mut reader) = tokio::io::duplex(1);
		let drain = tokio::spawn(async move {
			let mut byte = [0; 1];
			while reader.read_exact(&mut byte).await.is_ok() {
				tokio::time::sleep(Duration::from_millis(100)).await;
			}
		});
		let start = Instant::now();
		assert!(write_packets(&mut writer, &mut rx, &p).await.is_err());
		assert!(
			start.elapsed() >= Duration::from_millis(4900)
				&& start.elapsed() < Duration::from_millis(6000)
		);
		assert_eq!(p.bytes.available_permits(), M);
		drain.abort();
		let _ = drain.await;
	}
	struct PendingHandler {
		entered: Arc<tokio::sync::Notify>,
	}
	impl Handler for PendingHandler {
		fn handle<'a>(
			&'a self,
			_: &'static str,
			_: Route,
			_: &'a [Vec<u8>],
		) -> std::pin::Pin<Box<dyn std::future::Future<Output = Res<()>> + Send + 'a>> {
			Box::pin(async move {
				self.entered.notify_one();
				std::future::pending().await
			})
		}
	}
	#[tokio::test]
	async fn borrowed_application_holds_credit_and_shutdown_cancels_it() {
		let entered = Arc::new(tokio::sync::Notify::new());
		let mut server = Server::bind(
			["127.0.0.1:0".parse().unwrap(); 5],
			Arc::new(PendingHandler {
				entered: entered.clone(),
			}),
		)
		.await
		.unwrap();
		let publisher = server.publisher();
		let mut socket = TcpStream::connect(server.addresses()[0]).await.unwrap();
		let mut greeting = [0; 64];
		greeting[0] = 255;
		greeting[9] = 127;
		greeting[10] = 3;
		greeting[12..16].copy_from_slice(b"NULL");
		socket.write_all(&greeting).await.unwrap();
		socket.read_exact(&mut greeting).await.unwrap();
		let ready = metadata("DEALER");
		socket.write_all(&[4, ready.len() as u8]).await.unwrap();
		socket.write_all(&ready).await.unwrap();
		let (_, n) = header(&mut socket).await.unwrap();
		body(&mut socket, n, 8192).await.unwrap();
		socket
			.write_all(&encode(&[vec![42; 65536]]).unwrap())
			.await
			.unwrap();
		timeout(Duration::from_secs(2), entered.notified())
			.await
			.unwrap();
		let ep = &server.state.endpoints[0];
		assert_eq!(ep.incoming.available_permits(), 8 * M - 65536);
		assert_eq!(ep.slots.available_permits(), 7);
		server.shutdown().await.unwrap();
		assert_eq!(
			server.state.endpoints[0].incoming.available_permits(),
			8 * M
		);
		assert_eq!(server.state.endpoints[0].slots.available_permits(), 8);
		assert!(publisher.publish(&[b"after-shutdown".to_vec()]).is_err());
		assert_eq!(
			socket.read_u8().await.unwrap_err().kind(),
			std::io::ErrorKind::UnexpectedEof
		);
	}
	#[test]
	fn unwinding_retires_the_connection_generation() {
		let ep = ep();
		let stats = Stats::default();
		let (peer, _rx) = peer(M);
		ep.peers
			.lock()
			.unwrap()
			.insert(peer.id.clone(), peer.clone());
		let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
			let _connected = Connected {
				peer: &peer,
				endpoint: &ep,
				stats: &stats,
			};
			panic!("application future failed");
		}));
		assert!(result.is_err());
		assert!(ep.peers.lock().unwrap().is_empty());
		assert!(*peer.stop.borrow());
		assert_eq!(stats.closed.load(SeqCst), 1);
		assert!(enqueue(&peer, Arc::new(vec![1]), None).is_err());
	}
}
