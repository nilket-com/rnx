//! Borrow the execution heap while using the existing output policy and tokens.
use super::*;
use rune::experimental_core::heap::{ValueView, ViewKind};

pub(crate) fn render(
	node: ValueView<'_, '_>,
	fields: Option<&Fields>,
	limits: &Limits,
	styled: bool,
) -> Result<String, String> {
	let mut renderer = Renderer {
		out: String::new(),
		spans: Vec::new(),
		styled,
		fields,
		limits,
		path: Vec::new(),
		truncated: false,
		work: Work::default(),
		failure: None,
	};
	renderer.node(Node::Execution(node), 1)?;
	if let Some(reason) = renderer.failure {
		return Err(reason);
	}
	if renderer.truncated {
		renderer.out.push_str(&format!(
			" …(output truncated at {} bytes)",
			limits.total_bytes
		));
	}
	Ok(presentation::paint(&renderer.out, &renderer.spans, styled).into_owned())
}

pub(crate) fn error_text(
	node: ValueView<'_, '_>,
	fields: Option<&Fields>,
) -> Result<String, String> {
	let limits = Limits::preview();
	if let ViewKind::String(text) = node
		.kind()
		.map_err(|e| format!("{}: {:?}", e.operation, e.kind))?
	{
		let mut out = String::new();
		let consumed = terminal_safe_into(text, limits.string_bytes, &mut out);
		if consumed < text.len() {
			out.push_str(&format!("…(+{} bytes)", text.len() - consumed));
		}
		Ok(out)
	} else {
		render(node, fields, &limits, false)
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use rune::experimental_core::{
		execute::Executor,
		frontend,
		heap::{FormatLimits, Limits as HeapLimits, Store, ViewLimits},
		value::Type,
	};
	use rune::{Context, Diagnostics, Hash, Source, Sources};
	fn sources(text: &str) -> Sources {
		let mut sources = Sources::new();
		sources.insert(Source::memory(text).unwrap()).unwrap();
		sources
	}
	#[test]
	fn borrowed_shapes_share_legacy_rendering_for_full_preview_and_styled_output() {
		let context = Context::with_default_modules().unwrap();
		for body in [
			"i64",
			"()",
			"true",
			"-7",
			"2.5",
			"-0.0",
			"'界'",
			"\"hello\\n界\\u{1b}\"",
			"[1,2,3]",
			"(1,)",
			"(1,2)",
			"#{z:2,a:1}",
			"Some(None)",
			"Ok(Err(\"error\"))",
			"||42",
			"1..4",
			"[1,2].iter()",
			"std::cmp::Ordering::Less",
			"struct P;P",
			"struct P;P()",
			"struct PPair(a,b);PPair(1,2)",
			"struct PFields{value}PFields{value:3}",
			"enum E{V{value}}E::V{value:3}",
			"let a=[1,2];[a,a]",
			"let a=[];a.push(a);a",
			"let a=[];let b=[];a.push(b);b.push(a);a",
			"let a=[1];drop(a);a",
			"let a=\"ab\";drop(a);a",
			"let a=#{f:[1]};drop(a);a",
			"let a=([1],);drop(a);a",
			"let a=Some([1]);drop(a);a",
			"let a=Ok([1]);drop(a);a",
			"struct P{f}let a=P{f:[1]};drop(a);a",
			"let a=[1];let b=[a];drop(a);b",
			"let a=[1];drop(a);Err(a)",
			"let a=[1,2].iter();drop(a);a",
			"let a=(0..4).iter();drop(a);a",
		] {
			let text = format!("pub fn main(args){{{body}}}");
			let mut legacy = rune::prepare(&mut sources(&text))
				.with_context(&context)
				.build_vm()
				.unwrap();
			let legacy = legacy
				.call(["main"], (rune::to_value(Vec::<String>::new()).unwrap(),))
				.unwrap();
			let program =
				frontend::compile(&mut sources(&text), &context, &mut Diagnostics::new(), 1)
					.unwrap();
			let mut fields = Fields::default();
			crate::declared::into_fields(&text, &mut fields);
			for native in [false, true] {
				let mut store = Store::new(HeapLimits::default()).unwrap();
				let args = store.root_list(Type::Vec, &[]).unwrap();
				let mut executor = Executor::new().unwrap();
				executor.set_native_enabled(native);
				executor.set_format_limits(FormatLimits::permissive());
				executor
					.set_host_runtime(Some(
						rune::sync::Arc::try_new(context.runtime().unwrap()).unwrap(),
					))
					.unwrap();
				let root = executor
					.prepare(
						&program,
						&mut store,
						program.function(Hash::type_hash(["main"])).unwrap(),
						&[&args],
					)
					.unwrap()
					.invoke(100_000)
					.unwrap();
				for limits in [Limits::preview(), Limits::complete()] {
					for styled in [false, true] {
						let view = store
							.root_view(
								&root,
								ViewLimits {
									nodes: usize::MAX,
									depth: usize::MAX,
								},
							)
							.unwrap();
						let actual =
							render(view.root().unwrap(), Some(&fields), &limits, styled).unwrap();
						let expected = render_styled(&legacy, Some(&fields), &limits, styled);
						assert_eq!(actual, expected, "{body}; native={native}, styled={styled}");
					}
				}
				store.root_release(root).unwrap();
				store.root_release(args).unwrap();
				store.close().unwrap();
			}
		}
	}
}
