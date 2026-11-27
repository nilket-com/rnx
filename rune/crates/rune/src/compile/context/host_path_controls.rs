//! Metadata needed to exercise installed qualified functions in the cross control.
use super::*;
use std::{
	format,
	string::{String, ToString},
	vec::Vec,
};

impl Context {
	/// Visit each installed, named function with its concrete source path,
	/// qualified receiver, documented argument types and async calling convention.
	#[doc(hidden)]
	pub fn visit_host_path_controls(
		&self,
		mut visit: impl FnMut(Hash, String, Option<String>, Option<Vec<String>>, bool),
	) {
		let type_name = |hash: Hash| {
			self.types
				.get(&hash)
				.map(|ty| ty.item.to_string())
				.unwrap_or_else(|| hash.to_string())
		};
		for meta in &self.meta {
			let meta::Kind::Function {
				signature,
				container,
				parameters,
				parameter_types,
				..
			} = &meta.kind
			else {
				continue;
			};
			let Some(item) = &meta.item else { continue };
			let Some(handler) = self.functions.get(&meta.hash) else {
				continue;
			};
			let named = Hash::type_hash(item).with_generics(*parameters);
			assert!(
				self.functions
					.get(&named)
					.is_some_and(|target| target.same_allocation(handler)),
				"qualified and associated handlers differ for {item}: {named} vs {}",
				meta.hash
			);
			let mut path = item.to_string();
			let owner = container.map(|hash| {
				let ty = &self.types[&hash];
				let mut name = ty.item.to_string();
				if ty.type_parameters != Hash::EMPTY {
					let parameter = self
						.types
						.values()
						.find(|param| Hash::parameters([param.hash]) == ty.type_parameters)
						.expect("installed receiver with unclassified type parameters");
					name.push_str(&format!("::<{}>", parameter.item));
				}
				let (_, method) = path.rsplit_once("::").expect("associated path");
				path = format!("{name}::{method}");
				name
			});
			if !parameter_types.is_empty() {
				let args = parameter_types
					.iter()
					.map(|h| type_name(*h))
					.collect::<Vec<_>>();
				path.push_str(&format!("::<{}>", args.join(",")));
			}
			let args = signature
				.arguments
				.as_ref()
				.map(|args| args.iter().map(|arg| type_name(arg.base)).collect());
			visit(meta.hash, path, owner, args, signature.is_async);
		}
	}
}
