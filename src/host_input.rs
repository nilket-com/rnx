//! ADR0275: one parser reads either a borrowed stock value or a bounded packet.
use rune::{
	Value,
	host::{OwnedTreeInput, TreeKind},
	runtime::{Bytes, Object, Vec as RuneVec},
};

#[derive(Clone, Copy)]
pub(crate) enum Input<'a> {
	Legacy(&'a Value),
	Owned(&'a OwnedTreeInput, usize),
}

#[derive(Clone, Copy)]
pub(crate) enum Fields<'a> {
	Legacy(&'a Object),
	Input(Input<'a>),
}
impl Fields<'_> {
	pub(crate) fn field<T>(
		self,
		name: &str,
		f: impl FnOnce(Option<Input<'_>>) -> Result<T, String>,
	) -> Result<T, String> {
		match self {
			Self::Legacy(object) => f(object.get(name).map(Input::Legacy)),
			Self::Input(input) => input.field(name, "expected an object", f),
		}
	}
}
impl<'a> Input<'a> {
	pub(crate) fn owned(tree: &'a OwnedTreeInput) -> Self {
		match tree.leaf() {
			Some(value) => Self::Legacy(value),
			None => Self::Owned(tree, 0),
		}
	}
	fn node(self) -> Result<&'a rune::host::TreeNode, String> {
		let Self::Owned(tree, id) = self else {
			return Err("invalid input view".into());
		};
		tree.node(id)
			.ok_or_else(|| "invalid owned host input node".into())
	}
	pub(crate) fn string(self) -> Result<String, String> {
		match self {
			Self::Legacy(value) => value
				.borrow_string_ref()
				.map(|s| s.to_string())
				.map_err(|_| "expected a String".into()),
			Self::Owned(_, _) => match self.node()?.kind() {
				TreeKind::String(s) => Ok(s.as_str().to_owned()),
				_ => Err("expected a String".into()),
			},
		}
	}
	pub(crate) fn with_bytes<T>(
		self,
		not_bytes: &str,
		f: impl FnOnce(&[u8]) -> Result<T, String>,
	) -> Result<T, String> {
		match self {
			Self::Legacy(value) => {
				if let Ok(text) = value.borrow_string_ref() {
					return f(text.as_bytes());
				}
				let bytes = value
					.borrow_ref::<Bytes>()
					.map_err(|_| not_bytes.to_owned())?;
				f(&bytes)
			}
			Self::Owned(_, _) => match self.node()?.kind() {
				TreeKind::String(s) => f(s.as_bytes()),
				_ => Err(not_bytes.to_owned()),
			},
		}
	}
	pub(crate) fn is_bytes(self) -> bool {
		match self {
			Self::Legacy(v) => v.borrow_string_ref().is_ok() || v.borrow_ref::<Bytes>().is_ok(),
			Self::Owned(_, _) => self
				.node()
				.is_ok_and(|n| matches!(n.kind(), TreeKind::String(_))),
		}
	}
	pub(crate) fn copy_bytes(self, not_bytes: &str) -> Result<Value, String> {
		match self {
			Self::Legacy(value) if value.borrow_ref::<Bytes>().is_ok() => {
				self.with_bytes(not_bytes, |b| {
					rune::to_value(Bytes::from_slice(b).map_err(|e| e.to_string())?)
						.map_err(|e| e.to_string())
				})
			}
			_ => {
				let text = self.string().map_err(|_| not_bytes.to_owned())?;
				rune::to_value(text).map_err(|e| e.to_string())
			}
		}
	}
	fn scalar(self) -> Option<&'a Value> {
		match self {
			Self::Legacy(v) => Some(v),
			Self::Owned(_, _) => match self.node().ok()?.kind() {
				TreeKind::Scalar(v) => Some(v),
				_ => None,
			},
		}
	}
	pub(crate) fn integer<T>(self) -> Result<T, String>
	where
		T: TryFrom<i64> + TryFrom<u64>,
	{
		if let Some(value) = self.scalar() {
			return value.as_integer().map_err(|e| e.to_string());
		}
		Err(self
			.node()?
			.expected_number_error()
			.map_err(|e| e.to_string())?
			.to_string())
	}
	pub(crate) fn boolean(self) -> Option<bool> {
		rune::from_value::<bool>(self.scalar()?.clone()).ok()
	}
	pub(crate) fn none(self) -> bool {
		match self {
			Self::Legacy(v) => v.borrow_ref::<Option<Value>>().is_ok_and(|v| v.is_none()),
			Self::Owned(_, _) => self
				.node()
				.is_ok_and(|n| matches!(n.kind(), TreeKind::Option(false))),
		}
	}
	pub(crate) fn require_object(self) -> Result<(), rune::runtime::VmError> {
		let actual = match self {
			Self::Legacy(value) => {
				if value.borrow_ref::<Object>().is_ok() {
					return Ok(());
				}
				value.type_info()
			}
			Self::Owned(_, _) => {
				let node = self.node().map_err(rune::runtime::VmError::panic)?;
				if matches!(node.kind(), TreeKind::Object) {
					return Ok(());
				}
				rune::alloc::clone::TryClone::try_clone(node.type_info())
					.map_err(rune::runtime::VmError::from)?
			}
		};
		Err(rune::runtime::VmError::expected::<Object>(actual))
	}

	pub(crate) fn vector_len(self, error: &str) -> Result<usize, String> {
		match self {
			Self::Legacy(v) => v
				.borrow_ref::<RuneVec>()
				.map(|v| v.len())
				.map_err(|_| error.to_owned()),
			Self::Owned(_, _) => {
				let n = self.node()?;
				if matches!(n.kind(), TreeKind::Vec) {
					Ok(n.len())
				} else {
					Err(error.into())
				}
			}
		}
	}
	pub(crate) fn object_len(self, error: &str) -> Result<usize, String> {
		match self {
			Self::Legacy(v) => v
				.borrow_ref::<Object>()
				.map(|v| v.len())
				.map_err(|_| error.to_owned()),
			Self::Owned(_, _) => {
				let n = self.node()?;
				if matches!(n.kind(), TreeKind::Object) {
					Ok(n.len())
				} else {
					Err(error.into())
				}
			}
		}
	}
	pub(crate) fn vector(
		self,
		error: &str,
		mut f: impl FnMut(usize, Input<'_>) -> Result<(), String>,
	) -> Result<usize, String> {
		match self {
			Self::Legacy(value) => {
				let values = value
					.borrow_ref::<RuneVec>()
					.map_err(|_| error.to_owned())?;
				for (i, v) in values.iter().enumerate() {
					f(i, Input::Legacy(v))?;
				}
				Ok(values.len())
			}
			Self::Owned(tree, _) => {
				let node = self.node()?;
				if !matches!(node.kind(), TreeKind::Vec) {
					return Err(error.into());
				}
				if node.frontier() || node.cycle() {
					return Err(error.into());
				}
				for i in 0..node.len() {
					let (_, child) = node.child(i).ok_or("owned host input frontier")?;
					f(i, Self::Owned(tree, child))?;
				}
				Ok(node.len())
			}
		}
	}
	pub(crate) fn object(
		self,
		error: &str,
		mut f: impl FnMut(&str, Input<'_>) -> Result<(), String>,
	) -> Result<usize, String> {
		match self {
			Self::Legacy(value) => {
				let fields = value.borrow_ref::<Object>().map_err(|_| error.to_owned())?;
				for (key, v) in fields.iter() {
					f(key.as_str(), Input::Legacy(v))?;
				}
				Ok(fields.len())
			}
			Self::Owned(tree, _) => {
				let mut node = self.node()?;
				if !matches!(node.kind(), TreeKind::Object) {
					return Err(error.into());
				}
				// The parsers visit at most options -> env. A cycle there points
				// to the root options, whose fields remain available in the packet.
				if node.cycle() {
					node = tree.node(0).ok_or("owned host input cycle")?;
				}
				if node.frontier() {
					return Err(error.into());
				}
				for i in 0..node.len() {
					let (key, child) = node.child(i).ok_or("owned host input frontier")?;
					f(
						key.ok_or("owned host input object key")?,
						Self::Owned(tree, child),
					)?;
				}
				Ok(node.len())
			}
		}
	}
	pub(crate) fn field<T>(
		self,
		name: &str,
		error: &str,
		f: impl FnOnce(Option<Input<'_>>) -> Result<T, String>,
	) -> Result<T, String> {
		match self {
			Self::Legacy(v) => {
				let fields = v.borrow_ref::<Object>().map_err(|_| error.to_owned())?;
				f(fields.get(name).map(Input::Legacy))
			}
			Self::Owned(tree, _) => {
				let node = self.node()?;
				if !matches!(node.kind(), TreeKind::Object) {
					return Err(error.into());
				}
				for i in 0..node.len() {
					let (key, child) = node.child(i).ok_or("owned host input frontier")?;
					if key == Some(name) {
						return f(Some(Self::Owned(tree, child)));
					}
				}
				f(None)
			}
		}
	}
}
