//! Rust AST and macro-token inspection. No regexp or source-line-count oracle.
use super::Result;
use quote::{ToTokens, quote};
use serde::Serialize;
use std::{
	collections::BTreeSet,
	path::{Path, PathBuf},
};
use syn::{
	Attribute, Expr, Item, UseTree,
	visit::{self, Visit},
};
#[derive(Clone, Debug, Serialize)]
pub(super) struct Site {
	pub file: String,
	pub owner: String,
	pub tokens: String,
}
#[derive(Clone, Debug, Serialize)]
pub(super) struct FunctionShape {
	pub file: String,
	pub name: String,
	pub signature: String,
}
#[derive(Clone, Debug, Serialize)]
pub(super) struct Registration {
	pub file: String,
	pub owner: String,
	pub path: String,
	pub class: String,
	pub declaration: String,
}
fn excluded(attrs: &[Attribute]) -> bool {
	attrs.iter().any(|a| {
		a.path().is_ident("test")
			|| (a.path().is_ident("cfg") && {
				let s = a.meta.to_token_stream().to_string();
				(s.contains("test-support") || s.contains("test")) && !s.contains("not (test)")
			})
	})
}
fn walk(dir: &Path, files: &mut Vec<PathBuf>) -> Result<()> {
	if !dir.exists() {
		return Ok(());
	}
	for entry in std::fs::read_dir(dir)? {
		let path = entry?.path();
		let meta = std::fs::symlink_metadata(&path)?;
		if meta.file_type().is_symlink() {
			continue;
		}
		if meta.is_dir() {
			if !["target", "tests", "bin"].contains(&path.file_name().unwrap().to_str().unwrap()) {
				walk(&path, files)?;
			}
		} else if path.extension().is_some_and(|x| x == "rs") {
			files.push(path);
		}
	}
	Ok(())
}
fn files(root: &Path) -> Result<Vec<PathBuf>> {
	let mut files = Vec::new();
	for dir in ["src", "jupyter/src", "adapters", "servers"] {
		walk(&root.join(dir), &mut files)?;
	}
	files.push(root.join("tools/project/src/generate.rs"));
	files.push(root.join("tools/project/src/workflow.rs"));
	files.push(root.join("tools/project/src/workflow/git_sources.rs"));
	let mut skip = Vec::new();
	for p in &files {
		let tree = syn::parse_file(&std::fs::read_to_string(p)?)?;
		for item in &tree.items {
			let Item::Mod(m) = item else {
				continue;
			};
			if !excluded(&m.attrs) || m.content.is_some() {
				continue;
			}
			let dir = if ["lib", "main", "mod"]
				.iter()
				.any(|n| p.file_stem().unwrap() == *n)
			{
				p.parent().unwrap().to_path_buf()
			} else {
				p.parent().unwrap().join(p.file_stem().unwrap())
			};
			skip.push(dir.join(m.ident.to_string()));
			skip.push(dir.join(format!("{}.rs", m.ident)));
		}
	}
	files.retain(|p| {
		!skip.iter().any(|s| p == s || p.starts_with(s))
			&& !p
				.file_name()
				.unwrap()
				.to_str()
				.unwrap()
				.ends_with("_tests.rs")
			&& !p.ends_with("oracle.rs")
			&& !p.ends_with("fixtures.rs")
			&& !p.ends_with("test_main.rs")
	});
	files.sort();
	files.dedup();
	Ok(files)
}
struct Scan {
	file: String,
	owner: String,
	sites: Vec<Site>,
	registrations: Vec<Registration>,
	shapes: Vec<FunctionShape>,
	templates: Vec<Registration>,
	vm_aliases: BTreeSet<String>,
}
impl Scan {
	fn new(file: String) -> Self {
		Self {
			file,
			owner: String::new(),
			sites: Vec::new(),
			registrations: Vec::new(),
			shapes: Vec::new(),
			templates: Vec::new(),
			vm_aliases: BTreeSet::from(["Vm".into()]),
		}
	}
	fn site(&mut self, tokens: String) {
		self.sites.push(Site {
			file: self.file.clone(),
			owner: self.owner.clone(),
			tokens,
		});
	}
	fn prefix(&self) -> Option<&str> {
		let p = self.file.strip_prefix("src/")?.strip_suffix(".rs")?;
		[
			"json",
			"io",
			"process",
			"fs",
			"path",
			"time",
			"text",
			"env",
			"http",
			"web",
			"interchange",
		]
		.contains(&p)
		.then_some(p)
	}
	fn registration(&mut self, name: String, class: &str, declaration: String) {
		let path = if let Some(prefix) = self.prefix() {
			if prefix == "interchange" {
				format!("interchange::Dense::{name}")
			} else {
				format!("{prefix}::{name}")
			}
		} else {
			name
		};
		self.registrations.push(Registration {
			file: self.file.clone(),
			owner: self.owner.clone(),
			path,
			class: class.into(),
			declaration,
		});
	}
}
impl<'ast> Visit<'ast> for Scan {
	fn visit_item_fn(&mut self, n: &'ast syn::ItemFn) {
		if excluded(&n.attrs) {
			return;
		}
		let old = std::mem::replace(&mut self.owner, n.sig.ident.to_string());
		self.shapes.push(FunctionShape {
			file: self.file.clone(),
			name: n.sig.ident.to_string(),
			signature: n.sig.to_token_stream().to_string(),
		});
		self.site(n.to_token_stream().to_string());
		visit::visit_item_fn(self, n);
		self.owner = old;
	}
	fn visit_impl_item_fn(&mut self, n: &'ast syn::ImplItemFn) {
		if excluded(&n.attrs) {
			return;
		}
		let old = std::mem::replace(&mut self.owner, n.sig.ident.to_string());
		self.shapes.push(FunctionShape {
			file: self.file.clone(),
			name: n.sig.ident.to_string(),
			signature: n.sig.to_token_stream().to_string(),
		});
		self.site(n.to_token_stream().to_string());
		visit::visit_impl_item_fn(self, n);
		self.owner = old;
	}
	fn visit_item_mod(&mut self, n: &'ast syn::ItemMod) {
		if !excluded(&n.attrs) {
			visit::visit_item_mod(self, n);
		}
	}
	fn visit_item_impl(&mut self, n: &'ast syn::ItemImpl) {
		if !excluded(&n.attrs) {
			visit::visit_item_impl(self, n);
		}
	}
	fn visit_item_use(&mut self, n: &'ast syn::ItemUse) {
		fn aliases(tree: &UseTree, prefix: &str, out: &mut BTreeSet<String>) {
			match tree {
				UseTree::Path(p) => aliases(&p.tree, &format!("{prefix}{}::", p.ident), out),
				UseTree::Rename(r) if r.ident == "Vm" && prefix.contains("rune") => {
					out.insert(r.rename.to_string());
				}
				UseTree::Group(g) => {
					for t in &g.items {
						aliases(t, prefix, out)
					}
				}
				_ => {}
			}
		}
		aliases(&n.tree, "", &mut self.vm_aliases);
	}
	fn visit_expr_call(&mut self, n: &'ast syn::ExprCall) {
		if excluded(&n.attrs) {
			return;
		}
		if let Expr::Path(p) = n.func.as_ref() {
			self.site(format!(
				"CALL {}",
				p.path.to_token_stream().to_string().replace(' ', "")
			));
			let ids: Vec<_> = p
				.path
				.segments
				.iter()
				.map(|s| s.ident.to_string())
				.collect();
			if ids.len() >= 2
				&& self.vm_aliases.contains(&ids[ids.len() - 2])
				&& ["new", "without_runtime"].contains(&ids[ids.len() - 1].as_str())
			{
				self.site(format!("VM-CONSTRUCTOR {}", p.path.to_token_stream()));
			}
		}
		visit::visit_expr_call(self, n);
	}
	fn visit_expr_method_call(&mut self, n: &'ast syn::ExprMethodCall) {
		if excluded(&n.attrs) {
			return;
		}
		let method = n.method.to_string();
		self.site(format!("METHOD {method}"));
		if [
			"function",
			"function_meta",
			"raw_function",
			"associated_function",
			"host_function",
			"host_function_with_output",
			"field_function",
			"index_function",
		]
		.contains(&method.as_str())
			// Module::function(name, handler) registers a host. Program::function
			// and RuntimeContext::function take one lookup key. The other APIs
			// retain their own signatures, including one-argument function_meta.
			&& (method != "function" || n.args.len() == 2)
		{
			let name = n
				.args
				.first()
				.map(|a| match a {
					Expr::Lit(syn::ExprLit {
						lit: syn::Lit::Str(s),
						..
					}) => s.value(),
					Expr::Reference(r) => format!(
						"<{}>",
						r.expr
							.to_token_stream()
							.to_string()
							.split(" :: ")
							.last()
							.unwrap()
					),
					_ => a.to_token_stream().to_string(),
				})
				.unwrap_or_default();
			let class = match method.as_str() {
				"host_function" => "typed",
				"host_function_with_output" => "dynamic-output",
				_ => "undeclared",
			};
			self.registration(name, class, n.to_token_stream().to_string());
		}
		visit::visit_expr_method_call(self, n);
	}
	fn visit_macro(&mut self, n: &'ast syn::Macro) {
		if !n.path.is_ident("macro_rules") {
			// select! and similar DSLs contain Rust call expressions that syn
			// deliberately leaves opaque. Inspect token groups, never literal
			// text, and require the dot/name/parenthesized-arguments call shape.
			fn methods(scan: &mut Scan, stream: proc_macro2::TokenStream) {
				let tokens: Vec<_> = stream.into_iter().collect();
				for triple in tokens.windows(3) {
					if let [
						proc_macro2::TokenTree::Punct(dot),
						proc_macro2::TokenTree::Ident(name),
						proc_macro2::TokenTree::Group(args),
					] = triple && dot.as_char() == '.'
						&& args.delimiter() == proc_macro2::Delimiter::Parenthesis
					{
						scan.site(format!("METHOD {name}"));
					}
				}
				for token in tokens {
					if let proc_macro2::TokenTree::Group(group) = token {
						methods(scan, group.stream());
					}
				}
			}
			methods(self, n.tokens.clone());
		}
		if n.path.is_ident("macro_rules") {
			fn flatten(tokens: proc_macro2::TokenStream, out: &mut Vec<proc_macro2::TokenTree>) {
				for token in tokens {
					if let proc_macro2::TokenTree::Group(g) = token {
						flatten(g.stream(), out);
					} else {
						out.push(token);
					}
				}
			}
			let mut tokens = Vec::new();
			flatten(n.tokens.clone(), &mut tokens);
			for pair in tokens.windows(2) {
				if !matches!(&pair[0], proc_macro2::TokenTree::Punct(p) if p.as_char()=='.') {
					continue;
				}
				let proc_macro2::TokenTree::Ident(method) = &pair[1] else {
					continue;
				};
				let method = method.to_string();
				if [
					"function",
					"function_meta",
					"raw_function",
					"associated_function",
					"host_function",
					"host_function_with_output",
					"field_function",
					"index_function",
				]
				.contains(&method.as_str())
				{
					let class = match method.as_str() {
						"host_function" => "typed",
						"host_function_with_output" => "dynamic-output",
						_ => "undeclared",
					};
					self.templates.push(Registration {
						file: self.file.clone(),
						owner: self.owner.clone(),
						path: format!("<macro template: {method}>"),
						class: class.into(),
						declaration: n.tokens.to_string(),
					});
				}
			}
		}
		let name = n.path.segments.last().unwrap().ident.to_string();
		if ["reg", "register", "simple", "request", "assoc"].contains(&name.as_str()) {
			// Parse comma-separated macro arguments as syntax tokens. Macro definitions
			// are not invocations; their dollar metavariables never become registrations.
			let tokens: Vec<_> = n.tokens.clone().into_iter().collect();
			let mut class = "undeclared";
			let mut at = 0;
			if tokens.first().is_some_and(|t| t.to_string() == "execution") {
				class = "typed";
				at += 1;
			}
			if name == "assoc" {
				for t in &tokens {
					if let proc_macro2::TokenTree::Ident(i) = t {
						self.registration(i.to_string(), class, n.tokens.to_string());
					}
				}
			} else if let Some(t) = tokens.get(at) {
				let value = if let Ok(s) = syn::parse2::<syn::LitStr>(quote!(#t)) {
					Some(s.value())
				} else if matches!(t, proc_macro2::TokenTree::Ident(_)) {
					Some(t.to_string())
				} else {
					None
				};
				if let Some(v) = value {
					self.registration(v, class, n.tokens.to_string());
				}
			}
		}
	}
}
fn scan(root: &Path) -> Result<Vec<Scan>> {
	let mut all = Vec::new();
	for p in files(root)? {
		let file = p
			.strip_prefix(root)?
			.to_str()
			.ok_or("non-UTF8 source")?
			.replace('\\', "/");
		let mut s = Scan::new(file);
		s.visit_file(&syn::parse_file(&std::fs::read_to_string(p)?)?);
		all.push(s);
	}
	Ok(all)
}
pub(super) fn inventory(root: &Path) -> Result<Vec<Site>> {
	Ok(scan(root)?.into_iter().flat_map(|s| s.sites).collect())
}
pub(super) fn vm_constructors(root: &Path) -> Result<Vec<Site>> {
	Ok(inventory(root)?
		.into_iter()
		.filter(|s| s.tokens.starts_with("VM-CONSTRUCTOR"))
		.collect())
}
pub(super) fn registrations(root: &Path) -> Result<Vec<Registration>> {
	Ok(scan(root)?
		.into_iter()
		.flat_map(|s| s.registrations)
		.collect())
}
pub(super) fn registration_templates(root: &Path) -> Result<Vec<Registration>> {
	Ok(scan(root)?.into_iter().flat_map(|s| s.templates).collect())
}
pub(super) fn function_shapes(root: &Path) -> Result<Vec<FunctionShape>> {
	Ok(scan(root)?
		.into_iter()
		.flat_map(|s| s.shapes)
		.filter(|s| s.file.starts_with("src/"))
		.collect())
}
pub(super) fn self_check() {
	let source = r#"
use rune::Vm as Machine;
fn real(){
	let _=Machine::new(x,y);
	m.host_function("active",f);
	m.function("plain",f);
	m.function_meta(meta);
	program.function(rune::Hash::type_hash(["main"]));
	runtime.function(&hash);
	tokio::select! { item=next()=>{program.prepare(args);} }
	println!("program.prepare(fake)");
	macro_rules! unused {()=>{program.prepare(fake)}}
}
#[cfg(test)]fn hidden(){Vm::new(x,y);m.function("test",f);}
// m.function("comment",f)
	"#;
	let mut s = Scan::new("src/json.rs".into());
	s.visit_file(&syn::parse_file(source).unwrap());
	assert_eq!(
		s.sites
			.iter()
			.filter(|s| s.tokens.starts_with("VM-CONSTRUCTOR"))
			.count(),
		1
	);
	assert_eq!(
		s.sites
			.iter()
			.filter(|site| site.tokens == "METHOD prepare")
			.count(),
		1
	);
	assert_eq!(
		s.registrations
			.iter()
			.map(|registration| registration.path.as_str())
			.collect::<Vec<_>>(),
		["json::active", "json::plain", "json::meta"]
	);
}

#[test]
fn ast_controls_detect_aliases_and_ignore_test_registrations_and_comments() {
	self_check();
}
