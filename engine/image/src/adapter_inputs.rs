use crate::provider_abi as abi;
use rnx::rune::host::{ColumnSelection, FloatDtype, OwnedTreeInput, SelectionError, TreeKind};
use std::ffi::c_void;

unsafe extern "C" fn node(cookie: *const c_void, index: usize, out: *mut abi::Node) -> i32 {
	let tree = unsafe { &*cookie.cast::<OwnedTreeInput>() };
	let Some(n) = tree.node(index) else { return 1 };
	let (kind, text) = match n.kind() {
		TreeKind::Vec => (abi::SCHEMA_VECTOR, None),
		TreeKind::Tuple => (abi::SCHEMA_TUPLE, None),
		TreeKind::String(s) => (abi::SCHEMA_STRING, Some(s.as_str())),
		_ => (abi::SCHEMA_OTHER, None),
	};
	unsafe {
		out.write(abi::Node {
			kind,
			frontier: n.frontier().into(),
			children: n.len(),
			text: text.map(abi::Span::text).unwrap_or_default(),
		})
	};
	0
}
unsafe extern "C" fn child(cookie: *const c_void, index: usize, at: usize, out: *mut usize) -> i32 {
	let tree = unsafe { &*cookie.cast::<OwnedTreeInput>() };
	let Some((_, i)) = tree.node(index).and_then(|n| n.child(at)) else {
		return 1;
	};
	unsafe { out.write(i) };
	0
}
pub fn with_schema<T>(path: &str, tree: &OwnedTreeInput, f: impl FnOnce(&abi::Input) -> T) -> T {
	let schema = abi::Schema {
		cookie: (tree as *const OwnedTreeInput).cast(),
		node: Some(node),
		child: Some(child),
	};
	f(&abi::Input {
		schema,
		path: abi::Span::text(path),
		credit: tree.remaining_bytes().unwrap_or_default(),
		..Default::default()
	})
}
pub fn with_columns<T, E>(
	token: u64,
	select: &ColumnSelection,
	credit: usize,
	f: impl FnOnce(&abi::Input) -> Result<T, E>,
) -> Result<T, E>
where
	E: From<rnx::rune::host::OpaqueProducerLimit>,
{
	let mut input = abi::Input {
		receiver: token,
		credit,
		..Default::default()
	};
	let spans: Vec<_>;
	match &select.0 {
		Ok((names, dtype)) => {
			let limit = rnx::rune::host::OpaqueProducerLimit;
			// Both the wire spans and the provider's reconstructed selection are
			// live during conversion. The original packet is already charged.
			let copies = names
				.iter()
				.try_fold(0usize, |bytes, name| {
					bytes
						.checked_add(size_of::<abi::Span>() + size_of::<String>())?
						.checked_add(name.len())
				})
				.ok_or(limit)?;
			input.credit = credit
				.checked_sub(copies)
				.ok_or(rnx::rune::host::OpaqueProducerLimit)?;
			let mut storage = Vec::new();
			storage
				.try_reserve_exact(names.len())
				.map_err(|_| rnx::rune::host::OpaqueProducerLimit)?;
			storage.extend(names.iter().map(|s| abi::Span::text(s)));
			spans = storage;
			input.names = spans.as_ptr();
			input.count = spans.len();
			input.dtype = match dtype {
				FloatDtype::F32 => 32,
				FloatDtype::F64 => 64,
			};
		}
		Err(SelectionError::NotNames) => input.selection = 1,
		Err(SelectionError::Count(n)) => {
			input.selection = 2;
			input.count = *n;
		}
		Err(SelectionError::NameLength(n)) => {
			input.selection = 3;
			input.count = *n;
		}
		Err(SelectionError::Duplicate(s)) => {
			input.selection = 4;
			input.detail = abi::Span::text(s);
		}
		Err(SelectionError::Dtype(s)) => {
			input.selection = 5;
			input.detail = abi::Span::text(s);
		}
	}
	f(&input)
}
