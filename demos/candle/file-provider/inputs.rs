//! Read the borrowed packet. Schema children and their strings are valid
//! through invoke; no borrowed value is kept with the resulting DataFrame.
use super::Failure;
use crate::abi;
use rnx::rune::host::{ColumnSelection, FloatDtype, SelectionError};
use rnx_polars::image_provider::{SchemaKind, SchemaNode, SchemaTree};
unsafe fn span<'a, T>(p: *const T, n: usize) -> Result<&'a [T], Failure> {
	if n == 0 {
		return Ok(&[]);
	}
	if p.is_null() || !p.is_aligned() || n > isize::MAX as usize / size_of::<T>() {
		return Err(Failure::Protocol);
	}
	Ok(unsafe { std::slice::from_raw_parts(p, n) })
}
pub(super) fn text(s: &abi::Span) -> Result<&str, Failure> {
	std::str::from_utf8(unsafe { span(s.data, s.len)? }).map_err(|_| Failure::Protocol)
}
pub(super) struct Tree<'a>(pub &'a abi::Input);
impl SchemaTree for Tree<'_> {
	fn schema_node(&self, index: usize) -> Option<SchemaNode<'_>> {
		let tree = &self.0.schema;
		let mut node = abi::Node::default();
		if unsafe { (tree.node?)(tree.cookie, index, &raw mut node) } != 0 {
			return None;
		}
		let kind = match node.kind {
			abi::SCHEMA_VECTOR => SchemaKind::Vector,
			abi::SCHEMA_TUPLE => SchemaKind::Tuple,
			abi::SCHEMA_STRING => SchemaKind::Text(
				std::str::from_utf8(unsafe { span(node.text.data, node.text.len).ok()? }).ok()?,
			),
			abi::SCHEMA_OTHER => SchemaKind::Other,
			_ => return None,
		};
		Some(SchemaNode {
			kind,
			frontier: node.frontier != 0,
			children: node.children,
		})
	}
	fn child_index(&self, index: usize, at: usize) -> Option<usize> {
		let tree = &self.0.schema;
		let mut out = 0;
		(unsafe { (tree.child?)(tree.cookie, index, at, &raw mut out) } == 0).then_some(out)
	}
	fn credit(&self) -> Option<usize> {
		Some(self.0.credit)
	}
}
pub(super) fn columns(i: &abi::Input) -> Result<ColumnSelection, Failure> {
	Ok(ColumnSelection(match i.selection {
		0 => {
			let spans = unsafe { span(i.names, i.count)? };
			let mut names = Vec::new();
			names
				.try_reserve_exact(spans.len())
				.map_err(|_| Failure::Limit)?;
			for name in spans {
				names.push(text(name)?.to_owned())
			}
			Ok((
				names,
				match i.dtype {
					32 => FloatDtype::F32,
					64 => FloatDtype::F64,
					_ => return Err(Failure::Protocol),
				},
			))
		}
		1 => Err(SelectionError::NotNames),
		2 => Err(SelectionError::Count(i.count)),
		3 => Err(SelectionError::NameLength(i.count)),
		4 => Err(SelectionError::Duplicate(text(&i.detail)?.to_owned())),
		5 => Err(SelectionError::Dtype(text(&i.detail)?.to_owned())),
		_ => return Err(Failure::Protocol),
	}))
}
