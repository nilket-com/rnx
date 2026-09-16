//! One typed query per connection, registered through rnx's tracked lifecycle.
use futures_util::TryStreamExt;
use rnx::{Scope, rune};
use rune::runtime::{Object, Value, Vec as RuneVec};
use std::collections::HashSet;
use std::time::Duration;
use tokio_postgres::{Config, NoTls, config::SslMode, types::ToSql};

mod value;
use value::{Account, error, insert, string};

/// Install into the module named `postgres` using `Extensions::with_lifecycle`.
pub fn build(
	module: &mut rune::Module,
	scope: Scope,
) -> Result<Vec<(String, &'static str)>, String> {
	module
		.function(
			"query",
			move |url: &str, sql: &str, params: Value, options: Value| {
				// Borrow the caller's strings only for the native call, then
				// give the lazy tracked future its own snapshot. Neither binding
				// is consumed or kept borrowed across await.
				scope.track(query(url.to_owned(), sql.to_owned(), params, options))
			},
		)
		.build()
		.map_err(error)?;
	Ok(vec![(
		"postgres::query".into(),
		"query(url, sql, params, options).await -> Result<Object>: one typed statement, one connection; timeout_ms 1..90000 (default 30000), 10000 rows, 8 MiB logical payload; errors after dispatch may follow a committed write; no TLS or retry",
	)])
}

fn timeout(options: &Value) -> Result<u64, String> {
	let object = options
		.borrow_ref::<Object>()
		.map_err(|_| "options must be an object")?;
	let mut timeout = 30000;
	for (key, value) in object.iter() {
		if key != "timeout_ms" {
			return Err(format!("unknown option {key:?}"));
		}
		timeout = value
			.as_integer::<u64>()
			.map_err(|_| "timeout_ms must be an integer from 1 through 90000")?;
		if !(1..=90000).contains(&timeout) {
			return Err("timeout_ms must be an integer from 1 through 90000".into());
		}
	}
	Ok(timeout)
}

fn target(config: &Config) -> String {
	let database = config
		.get_dbname()
		.or(config.get_user())
		.unwrap_or("default database");
	let mut hosts: Vec<_> = config
		.get_hosts()
		.iter()
		.map(|host| match host {
			tokio_postgres::config::Host::Tcp(host) => host.clone(),
			#[cfg(unix)]
			tokio_postgres::config::Host::Unix(path) => path.display().to_string(),
		})
		.collect();
	if hosts.is_empty() {
		hosts.extend(config.get_hostaddrs().iter().map(ToString::to_string));
	}
	let host = if hosts.is_empty() {
		"default host".into()
	} else {
		hosts.join(", ")
	};
	format!("cannot query {database} at {host}: ")
}

#[cfg(test)]
mod tests {
	use super::*;
	#[test]
	fn error_target_names_all_configured_hosts_without_credentials() {
		let mut config = Config::new();
		config
			.dbname("db")
			.user("user")
			.password("secret")
			.host("one")
			.host("two");
		assert_eq!(target(&config), "cannot query db at one, two: ");
		let mut config = Config::new();
		config.dbname("db").hostaddr("127.0.0.1".parse().unwrap());
		assert_eq!(target(&config), "cannot query db at 127.0.0.1: ");
	}
}

fn database_error(e: tokio_postgres::Error) -> String {
	if let Some(db) = e.as_db_error() {
		format!("{} [{}]", db.message(), db.code().code())
	} else {
		// Display omits the URL and parameters. Include the I/O cause for a
		// useful connect refusal, without Debug's connection configuration.
		use std::error::Error;
		match e.source() {
			Some(cause) => format!("{e}: {cause}"),
			None => e.to_string(),
		}
	}
}

async fn query(url: String, sql: String, params: Value, options: Value) -> Result<Value, String> {
	let started = tokio::time::Instant::now();
	let mut config: Config = url.parse().map_err(|_| "cannot query unknown database at unknown host: invalid connection URL or sslmode option".to_owned())?;
	let prefix = target(&config);
	let password = config
		.get_password()
		.map(|p| String::from_utf8_lossy(p).into_owned());
	let result = async {
		let timeout_ms = timeout(&options)?;
		if config.get_ssl_mode() == SslMode::Require {
			return Err("sslmode=require is not supported; this adapter offers no TLS".into());
		}
		if sql.len() > 1024 * 1024 {
			return Err("SQL exceeds 1048576 bytes".into());
		}
		let mut account = Account::new();
		account.charge(sql.len(), 0)?;
		let params = value::params(params, &mut account)?;
		// Replace URL options: a supplied options string cannot undo the bound.
		config.options(format!("-c statement_timeout={timeout_ms}"));
		let deadline = started + Duration::from_millis(timeout_ms);
		tokio::time::timeout_at(deadline, async {
			let (client, connection) = config.connect(NoTls).await.map_err(database_error)?;
			let statement = async {
				let types: Vec<_> = params.iter().map(value::Param::ty).collect();
				let prepared = client
					.prepare_typed(&sql, &types)
					.await
					.map_err(database_error)?;
				if params.len() != prepared.params().len() {
					return Err(format!(
						"parameter count is {}, statement expects {}",
						params.len(),
						prepared.params().len()
					));
				}
				let mut names = HashSet::new();
				let mut columns =
					RuneVec::with_capacity(prepared.columns().len()).map_err(error)?;
				for column in prepared.columns() {
					if !names.insert(column.name()) {
						return Err(format!("duplicate column name {:?}", column.name()));
					}
					if !value::supported(column.type_()) {
						return Err(format!(
							"column {:?} has unsupported PostgreSQL type {}",
							column.name(),
							column.type_().name()
						));
					}
					columns.push(string(column.name())?).map_err(error)?;
				}
				let stream = client
					.query_raw(&prepared, params.iter().map(|p| p as &(dyn ToSql + Sync)))
					.await
					.map_err(database_error)?;
				tokio::pin!(stream);
				let mut rows = RuneVec::new();
				#[cfg(feature = "test-support")]
				let mut held = Vec::new();
				while let Some(row) = stream.try_next().await.map_err(database_error)? {
					#[cfg(feature = "test-support")]
					if testing::enabled() {
						// The commitment fixture returns exactly one row. This path
						// exists only in the test binary, activated explicitly there.
						if !held.is_empty() {
							return Err("test pause accepts one row only".into());
						}
						held.push(row);
						continue;
					}
					let row = value::row(row, &mut account, rows.len() + 1)?;
					rows.push(row).map_err(error)?;
				}
				#[cfg(feature = "test-support")]
				if testing::enabled() {
					testing::pause().await?;
					for row in held {
						rows.push(value::row(row, &mut account, rows.len() + 1)?)
							.map_err(error)?;
					}
				}
				let affected = stream.rows_affected().unwrap_or(0);
				let mut result = Object::with_capacity(3).map_err(error)?;
				insert(
					&mut result,
					"columns",
					Value::try_from(columns).map_err(error)?,
				)?;
				insert(&mut result, "rows", Value::try_from(rows).map_err(error)?)?;
				insert(&mut result, "affected", Value::from(affected))?;
				Value::try_from(result).map_err(error)
			};
			tokio::pin!(connection, statement);
			// Both futures are owned here. There is no task, abort handle or
			// cleanup turn; revoking Scope's inner future closes the descriptor.
			tokio::select! {
				result = &mut statement => result,
				result = &mut connection => match result {
					Err(e) => Err(database_error(e)),
					Ok(()) => Err("connection ended before query completed".into()),
				}
			}
		})
		.await
		.map_err(|_| format!("deadline of {timeout_ms} ms elapsed; completion may be ambiguous"))?
	}
	.await;
	result.map_err(|reason| {
		let message = prefix + &reason;
		match password.filter(|p| !p.is_empty()) {
			Some(password) => message.replace(&password, "[redacted]"),
			None => message,
		}
	})
}

/// Fixture-only activation. The ordinary executable never calls this.
#[cfg(feature = "test-support")]
pub mod testing {
	use std::sync::atomic::{AtomicBool, Ordering};
	static ENABLED: AtomicBool = AtomicBool::new(false);
	pub fn enable_pause() {
		ENABLED.store(true, Ordering::Relaxed);
	}
	pub(crate) fn enabled() -> bool {
		ENABLED.load(Ordering::Relaxed)
	}
	pub(crate) async fn pause() -> Result<(), String> {
		let path = std::path::PathBuf::from(
			std::env::var_os("RNX_PG_TEST_PAUSE").ok_or("missing test pause path")?,
		);
		std::fs::write(path.with_extension("ready"), b"command complete")
			.map_err(|e| e.to_string())?;
		while !path.with_extension("release").exists() {
			tokio::time::sleep(std::time::Duration::from_millis(5)).await;
		}
		Ok(())
	}
}
