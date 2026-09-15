//! The supported local connection-file subset, validated before any listener binds.
use crate::transport::Res;
use serde_json::Value;
use std::{
	fs::OpenOptions,
	io::Read,
	net::{IpAddr, SocketAddr},
	path::Path,
};
const CAP: usize = 64 * 1024;

/// Contains a secret key. Deliberately has no Debug implementation.
pub struct Connection {
	/// shell, control, stdin, heartbeat, IOPub, matching the transport endpoint order.
	pub addresses: [SocketAddr; 5],
	pub key: Vec<u8>,
}
impl Connection {
	pub fn read(path: &Path) -> Res<Self> {
		Self::read_inner(path).map_err(|e| {
			format!(
				"cannot read Jupyter connection file {}: {e}",
				path.display()
			)
			.into()
		})
	}
	fn read_inner(path: &Path) -> Res<Self> {
		let mut options = OpenOptions::new();
		options.read(true);
		#[cfg(unix)]
		{
			use std::os::unix::fs::OpenOptionsExt;
			options.custom_flags(libc::O_NONBLOCK);
		}
		let file = options.open(path)?;
		if !file.metadata()?.is_file() {
			return Err("expected a regular file".into());
		}
		let mut bytes = Vec::new();
		file.take((CAP + 1) as u64).read_to_end(&mut bytes)?;
		if bytes.len() > CAP {
			return Err("connection file exceeds 64 KiB".into());
		}
		Self::parse(&bytes)
	}
	fn parse(bytes: &[u8]) -> Res<Self> {
		if bytes.len() > CAP {
			return Err("connection file exceeds 64 KiB".into());
		}
		let v: Value = serde_json::from_slice(bytes)?;
		if !v.is_object() {
			return Err("connection file must be an object".into());
		}
		if v.get("registration_port").is_some() || v.get("registration_ip").is_some() {
			return Err("registration files are unsupported".into());
		}
		let string = |name: &str| -> Res<&str> {
			v[name]
				.as_str()
				.ok_or_else(|| format!("{name} must be a string").into())
		};
		if string("transport")? != "tcp" {
			return Err("transport must be tcp".into());
		}
		let ip: IpAddr = string("ip")?
			.parse()
			.map_err(|_| "ip must be a numeric loopback address")?;
		if !ip.is_loopback() {
			return Err("ip must be a numeric loopback address".into());
		}
		if string("signature_scheme")? != "hmac-sha256" {
			return Err("signature_scheme must be hmac-sha256".into());
		}
		let key = string("key")?.as_bytes().to_vec();
		let mut addresses = [SocketAddr::new(ip, 0); 5];
		for (i, name) in [
			"shell_port",
			"control_port",
			"stdin_port",
			"hb_port",
			"iopub_port",
		]
		.into_iter()
		.enumerate()
		{
			let port = v[name]
				.as_u64()
				.filter(|p| *p > 0 && *p <= u16::MAX as u64)
				.ok_or_else(|| format!("{name} must be an integer in 1..=65535"))?
				as u16;
			if addresses[..i].iter().any(|a| a.port() == port) {
				return Err(format!("{name} repeats another port").into());
			}
			addresses[i].set_port(port);
		}
		Ok(Self { addresses, key })
	}
}
#[cfg(test)]
mod tests {
	use super::*;
	use serde_json::json;
	fn valid() -> Value {
		json!({"transport":"tcp","ip":"127.0.0.1","signature_scheme":"hmac-sha256","key":"private","shell_port":1,"control_port":2,"stdin_port":3,"hb_port":4,"iopub_port":5,"future_extension":true})
	}
	fn parse(v: &Value) -> Res<Connection> {
		Connection::parse(&serde_json::to_vec(v).unwrap())
	}
	#[test]
	fn local_subset_and_typed_fields() {
		assert_eq!(parse(&valid()).unwrap().addresses[3].port(), 4);
		for (name, value) in [
			("ip", json!("0.0.0.0")),
			("ip", json!("localhost")),
			("ip", json!("192.0.2.1")),
			("transport", json!("ipc")),
			("signature_scheme", json!("hmac-sha1")),
			("key", json!(42)),
			("shell_port", json!(0)),
			("shell_port", json!(-1)),
			("shell_port", json!(1.5)),
			("shell_port", json!(65536)),
			("shell_port", json!(2)),
			("registration_port", json!(123)),
		] {
			let mut v = valid();
			v[name] = value;
			assert!(parse(&v).is_err(), "{name}");
		}
		let mut v = valid();
		v["ip"] = json!("::1");
		v["key"] = json!("");
		assert!(parse(&v).unwrap().key.is_empty());
		for name in ["transport", "ip", "signature_scheme", "key", "hb_port"] {
			let mut v = valid();
			v.as_object_mut().unwrap().remove(name);
			assert!(parse(&v).is_err());
		}
		let mut bytes = serde_json::to_vec(&valid()).unwrap();
		bytes.resize(CAP, b' ');
		assert!(Connection::parse(&bytes).is_ok());
		bytes.push(b' ');
		assert!(Connection::parse(&bytes).is_err());
	}
	#[test]
	fn regular_file_and_size_limit() {
		let dir = std::env::temp_dir().join(format!("rnx-jupyter-{}", uuid::Uuid::new_v4()));
		std::fs::create_dir(&dir).unwrap();
		let path = dir.join("connection.json");
		std::fs::write(&path, serde_json::to_vec(&valid()).unwrap()).unwrap();
		assert!(Connection::read(&path).is_ok());
		std::fs::write(&path, vec![b' '; CAP + 1]).unwrap();
		assert!(Connection::read(&path).is_err());
		assert!(Connection::read(&dir).is_err());
		#[cfg(unix)]
		{
			use std::os::unix::ffi::OsStrExt;
			let pipe = dir.join("fifo");
			let name = std::ffi::CString::new(pipe.as_os_str().as_bytes()).unwrap();
			assert_eq!(unsafe { libc::mkfifo(name.as_ptr(), 0o600) }, 0);
			assert!(
				Connection::read(&pipe)
					.err()
					.unwrap()
					.to_string()
					.contains("regular file")
			);
			std::fs::remove_file(pipe).unwrap();
		}
		std::fs::remove_file(path).unwrap();
		std::fs::remove_dir(dir).unwrap();
	}
}
