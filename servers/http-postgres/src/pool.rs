//! Server-owned pool. Two persistent connections per worker; no public API.
use super::*;
use tokio::sync::{OwnedSemaphorePermit, Semaphore};
use tokio_postgres::{Client, NoTls};
struct Connection {
	client: Arc<Client>,
	driver: tokio::task::JoinHandle<Result<(), tokio_postgres::Error>>,
	pid: i32,
}
pub struct Pool {
	idle: RefCell<Vec<Connection>>,
	permits: Arc<Semaphore>,
	drivers: Arc<AtomicUsize>,
	leased: Cell<usize>,
	worker: usize,
	url: String,
}
pub struct Lease {
	connection: Connection,
	permit: OwnedSemaphorePermit,
	owner: Rc<Pool>,
	id: usize,
}
impl Pool {
	pub async fn new(worker: usize) -> Rc<Self> {
		let p = Rc::new(Self {
			idle: RefCell::new(vec![]),
			permits: Arc::new(Semaphore::new(2)),
			drivers: Arc::new(AtomicUsize::new(0)),
			leased: Cell::new(0),
			worker,
			url: std::env::var("RNX_POOL_URL").unwrap(),
		});
		for _ in 0..2 {
			let c = p.connect().await.unwrap();
			p.idle.borrow_mut().push(c);
		}
		p.report("pool_ready");
		p
	}
	fn report(&self, kind: &str) {
		event(
			json!({"event":kind,"worker":self.worker,"drivers":self.drivers.load(Ordering::SeqCst),"leases":self.leased.get(),"idle":self.idle.borrow().len(),"runtime_tasks":tokio::runtime::Handle::current().metrics().num_alive_tasks()}),
		);
	}
	async fn connect(&self) -> Result<Connection, String> {
		let mut cfg: tokio_postgres::Config =
			self.url.parse().map_err(|_| "fixture URL".to_owned())?;
		cfg.application_name(format!("rnx-pool-{}", self.worker))
			.options("-c statement_timeout=800");
		let (client, connection) = tokio::time::timeout(Duration::from_secs(1), cfg.connect(NoTls))
			.await
			.map_err(|_| "connect timeout".to_owned())?
			.map_err(|e| e.to_string())?;
		let count = self.drivers.clone();
		count.fetch_add(1, Ordering::SeqCst);
		#[cfg(feature = "test-support")]
		let worker = self.worker;
		let driver = tokio::spawn(async move {
			struct Count(Arc<AtomicUsize>);
			impl Drop for Count {
				fn drop(&mut self) {
					self.0.fetch_sub(1, Ordering::SeqCst);
				}
			}
			let _count = Count(count);
			let result = connection.await;
			#[cfg(feature = "test-support")]
			if std::env::var_os("RNX_POOL_STALL_DRIVER").is_some() {
				event(json!({"event":"driver_stall","worker":worker}));
				std::future::pending::<()>().await;
			}
			result
		});
		let client = Arc::new(client);
		let pid = match client.query_one("SELECT pg_backend_pid()", &[]).await {
			Ok(r) => r.get(0),
			Err(e) => {
				drop(client);
				let _ = driver.await;
				return Err(e.to_string());
			}
		};
		event(json!({"event":"pool_connect","worker":self.worker,"pid":pid}));
		Ok(Connection {
			client,
			driver,
			pid,
		})
	}
	async fn retire(&self, c: Connection, id: usize, reason: &str) {
		let Connection {
			client,
			driver,
			pid,
		} = c;
		drop(client);
		let mut driver = driver;
		let result = match tokio::time::timeout(Duration::from_millis(1200), &mut driver).await {
			Ok(joined) => joined.expect("connection driver panicked"),
			Err(_) => {
				driver.abort();
				let joined = driver.await;
				event(
					json!({"event":"driver_retirement_failed","worker":self.worker,"id":id,"pid":pid,"reason":reason,"abort_joined":joined.as_ref().is_err_and(|e|e.is_cancelled()),"drivers_remaining":self.drivers.load(Ordering::SeqCst)}),
				);
				panic!("driver retirement deadline; forced abort, not clean shutdown");
			}
		};
		event(
			json!({"event":"pool_retire","worker":self.worker,"id":id,"pid":pid,"reason":reason,"driver_error":result.err().map(|e|e.to_string()),"drivers":self.drivers.load(Ordering::SeqCst)}),
		);
	}
	pub async fn checkout(
		self: &Rc<Self>,
		id: usize,
		cancel: &mut watch::Receiver<bool>,
	) -> Result<Lease, String> {
		// Only waiting for a permit is cancellable. After taking a connection,
		// the owner completes BEGIN and returns a lease that must be finished.
		if *cancel.borrow() {
			return Err("cancelled before lease".into());
		}
		let permit = tokio::select! {biased;
			_=cancel.changed()=>return Err("cancelled before lease".into()),
			p=self.permits.clone().acquire_owned()=>p.map_err(|e|e.to_string())?,
		};
		let old = self.idle.borrow_mut().pop();
		let c = match old {
			Some(c) if !c.client.is_closed() => c,
			Some(c) => {
				self.retire(c, id, "closed while idle").await;
				self.connect().await?
			}
			None => self.connect().await?,
		};
		if let Err(e) = c.client.batch_execute("BEGIN").await {
			self.retire(c, id, "begin failed").await;
			return Err(e.to_string());
		}
		self.leased.set(self.leased.get() + 1);
		event(json!({"event":"lease","worker":self.worker,"id":id,"pid":c.pid}));
		Ok(Lease {
			connection: c,
			permit,
			owner: self.clone(),
			id,
		})
	}
	pub async fn close(&self) {
		self.report("pool_closing");
		assert_eq!(self.leased.get(), 0);
		let idle = std::mem::take(&mut *self.idle.borrow_mut());
		for c in idle {
			self.retire(c, 0, "shutdown").await;
		}
		self.report("pool_closed");
		assert_eq!(self.drivers.load(Ordering::SeqCst), 0);
	}
}
impl Lease {
	pub fn client(&self) -> Arc<Client> {
		self.connection.client.clone()
	}
	pub async fn finish(self, success: bool) -> Result<(), String> {
		let Self {
			connection: c,
			permit,
			owner: p,
			id,
		} = self;
		let command = if success { "COMMIT" } else { "ROLLBACK" };
		event(json!({"event":"tx_finish_start","id":id,"pid":c.pid,"command":command}));
		let result =
			tokio::time::timeout(Duration::from_millis(1200), c.client.batch_execute(command))
				.await;
		let rejected = matches!(&result, Ok(Err(error)) if commit_rejected(error));
		let sqlstate = match &result {
			Ok(Err(error)) => error.as_db_error().map(|db| db.code().code().to_owned()),
			_ => None,
		};
		let error = match result {
			Ok(Ok(())) => None,
			Ok(Err(e)) => Some(e.to_string()),
			Err(_) => Some("cleanup deadline".into()),
		};
		let result = if let Some(detail) = error {
			let category = if success && rejected {
				"commit rejected; retired; no retry"
			} else if success {
				"ambiguous commit; no retry"
			} else {
				"rollback unacknowledged; retired"
			};
			event(
				json!({"event":"tx_finish_error","id":id,"pid":c.pid,"category":category,"sqlstate":sqlstate,"detail":detail}),
			);
			p.retire(c, id, category).await;
			// Replenish capacity, never replay the failed transaction.
			match p.connect().await {
				Ok(fresh) => p.idle.borrow_mut().push(fresh),
				Err(e) => event(json!({"event":"replacement_failed","id":id,"detail":e})),
			}
			Err(category.to_owned())
		} else {
			event(json!({"event":"tx_ack","id":id,"pid":c.pid,"command":command}));
			p.idle.borrow_mut().push(c);
			Ok(())
		};
		p.leased.set(p.leased.get() - 1);
		drop(permit);
		p.report("pool_state");
		result
	}
}

fn commit_rejected(error: &tokio_postgres::Error) -> bool {
	error.as_db_error().is_some_and(|db| {
		db.parsed_severity() == Some(tokio_postgres::error::Severity::Error)
			&& (db.code().code().starts_with("23") || matches!(db.code().code(), "40001" | "40P01"))
	})
}
