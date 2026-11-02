//! Bounded, nominal-specialized closure analysis. Native activations are
//! supplied by the same compiler as zero-receiver scalar recursion.
use super::{Shape, MAX_INSTRUCTIONS, MAX_SLOTS};
use crate::experimental_core::region::budget::{AnalysisPod, BVec, Budget};
use crate::experimental_core::{
	code::{Code, Op, INSTANCE_METHOD},
	nominal,
	value::{Nominal, Type, Value},
};
use crate::Hash;
use std::vec::Vec;

const MAX_FUNCTIONS: usize = 16;
pub(crate) const MAX_FIELDS: usize = 64;
const INT: u128 = 1;
const BOOL: u128 = 2;
const UNIT: u128 = 4;
const SELF: u128 = 8;
const ELEMENT: u128 = 16;
const HASH: u128 = 32;
const METHOD: u128 = 64;
const SCALAR: u128 = INT | BOOL | UNIT;

pub(crate) const HEADER_CELLS: usize = 5;
pub(crate) const ROOM_SLOTS: usize = 0;
pub(crate) const VECTOR_LEN: usize = 1;
pub(crate) const VECTOR_TAG: usize = 2;
pub(crate) const HELPER_CALLS: usize = 3;
pub(crate) const SELF_CALLS: usize = 4;

pub(crate) mod entry;

#[derive(Clone, Copy, Debug)]
pub(crate) struct Function {
	pub(crate) id: u32,
	pub(crate) start: usize,
	pub(crate) end: usize,
	pub(crate) slots: usize,
	pub(crate) parameters: usize,
}
#[derive(Clone, Copy, Debug)]
pub(crate) struct Field {
	pub(crate) function: u32,
	pub(crate) key: u32,
	/// 0 unused, 1 scalar, 2 the shared homogeneous Vec.
	pub(crate) kind: u8,
}
#[derive(Clone, Copy, Debug)]
pub(crate) struct Site {
	pub(crate) name: u64,
	pub(crate) target: u32,
}
impl AnalysisPod for Function {}
impl AnalysisPod for Field {}
impl AnalysisPod for Site {}
impl AnalysisPod for u128 {}
impl AnalysisPod for Nominal {}

pub(crate) struct Methods {
	pub(crate) code: u32,
	pub(crate) nominal: Nominal,
	pub(crate) engine: Shape,
	pub(crate) functions: Vec<Function>,
	pub(crate) fields: Vec<Field>,
	pub(crate) sites: Vec<Site>,
	/// A scalar store requires this vector element tag; swaps are tag-polymorphic.
	pub(crate) vector_tag: Option<u32>,
}
impl Methods {
	pub(crate) fn function(&self, pc: usize) -> Option<&Function> {
		self.functions.iter().find(|f| f.start == pc)
	}
	pub(crate) fn metadata(&self) -> usize {
		self.engine.metadata()
			+ self.functions.capacity() * core::mem::size_of::<Function>()
			+ self.fields.capacity() * core::mem::size_of::<Field>()
			+ self.sites.capacity() * core::mem::size_of::<Site>()
	}
	pub(crate) fn valid(&self) -> bool {
		let s = &self.engine;
		nominal::shape_copy(self.nominal) == Some(nominal::Shape::Named)
			&& !self.functions.is_empty()
			&& self.functions.len() <= MAX_FUNCTIONS
			&& !self.fields.is_empty()
			&& self.fields.len() <= MAX_FIELDS
			&& s.slots > 0
			&& s.slots <= MAX_SLOTS
			&& !s.ops.is_empty()
			&& s.ops.len() <= MAX_INSTRUCTIONS
			&& s.ops.len() == s.charged.len()
			&& s.ops.len() == s.reached.len()
			&& self.functions[0].start == 0
			&& self.functions[0].id == s.function
			&& self.functions.iter().all(|f| {
				f.start < f.end
					&& f.end <= s.ops.len()
					&& f.slots > 0 && f.slots <= s.slots
					&& f.parameters > 0
					&& f.parameters <= 4
					&& f.parameters <= f.slots
			}) && self.fields.iter().all(|f| f.kind == 1 || f.kind == 2)
			&& self.vector_tag.is_none_or(|t| t == 1 || t == 2)
	}
}

struct Work<'a>(&'a mut u64);
impl Work<'_> {
	fn pay(&mut self, n: usize) -> Option<()> {
		*self.0 = self.0.checked_sub(n as u64)?;
		Some(())
	}
}

fn names_equal(code: &Code, field: Field, function: u32, key: u32) -> Option<bool> {
	Some(
		code.function(field.function)?
			.strings
			.get(field.key as usize)?
			.as_str() == code.function(function)?.strings.get(key as usize)?.as_str(),
	)
}

fn kind(fields: &mut [Field], mask: u128, wanted: u8) -> Option<()> {
	let low = if wanted == 1 { SCALAR | ELEMENT } else { 0 };
	if mask == 0 || mask & ((1 << 7) - 1) & !low != 0 {
		return None;
	}
	for (i, field) in fields.iter_mut().enumerate() {
		if mask & (1u128 << (7 + i)) != 0 {
			if field.kind != 0 && field.kind != wanted {
				return None;
			}
			field.kind = wanted;
		}
	}
	Some(())
}

/// All loops/calls are bounded by the ordinary code and analysis budgets. The
/// mutable work allowance charges unsuccessful analyses as well as successful ones.
pub(crate) fn analyze(
	code: &Code,
	root: u32,
	budget: &Budget,
	allowance: &mut u64,
) -> Option<(Methods, usize)> {
	let mut work = Work(allowance);
	let root_fn = code.function(root)?;
	if root_fn.parameters == 0 || root_fn.parameters > 4 || root_fn.slots as usize > MAX_SLOTS {
		return None;
	}
	// No receiver metadata work for the ordinary scalar N1-N5 case.
	work.pay(root_fn.len())?;
	if !(0..root_fn.len()).any(|pc| {
		matches!(
			root_fn.cached_ref(pc).map(|i| i.op),
			Some(
				Op::Field { .. }
					| Op::SetField { .. }
					| Op::FieldGetMut { .. }
					| Op::FieldSetMut { .. }
			) | Some(Op::Call {
				function: INSTANCE_METHOD,
				..
			})
		)
	}) {
		return None;
	}
	let mut nominals = BVec::new(budget);
	let mut names = BVec::new(budget);
	for id in 0..code.functions() as u32 {
		let f = code.function(id)?;
		work.pay(f.len())?;
		for pc in 0..f.len() {
			match f.cached_ref(pc)?.op {
				Op::Record {
					tag: Type::Struct(n),
					..
				} if !nominals.contains(&n) => nominals.push(n).ok()?,
				Op::Literal {
					value: Value::U64(n),
					..
				} if !names.contains(&n) => names.push(n).ok()?,
				_ => (),
			}
		}
	}
	let mut associated = BVec::new(budget);
	for &n in nominals.iter() {
		if nominal::shape_copy(n) != Some(nominal::Shape::Named) {
			continue;
		}
		let receiver = Hash::new(nominal::hash_copy(n)?);
		for &name in names.iter() {
			work.pay(1)?;
			if code.region_script_method(Hash::associated_function(receiver, Hash::new(name)))
				== Some(root)
				&& !associated.contains(&n)
			{
				associated.push(n).ok()?;
			}
		}
	}
	let nominal = if associated.is_empty() {
		if nominals.len() != 1 {
			return None;
		}
		nominals[0]
	} else {
		if associated.len() != 1 {
			return None;
		}
		associated[0]
	};
	if nominal::shape_copy(nominal) != Some(nominal::Shape::Named) {
		return None;
	}
	let receiver = Hash::new(nominal::hash_copy(nominal)?);
	let mut functions = BVec::new(budget);
	functions
		.push(Function {
			id: root,
			start: 0,
			end: 0,
			slots: 0,
			parameters: 0,
		})
		.ok()?;
	let mut fields = BVec::new(budget);
	let mut sites = BVec::new(budget);
	let mut ops = BVec::new(budget);
	let mut charged = BVec::new(budget);
	let mut args = BVec::new(budget);
	let mut index = 0;
	let mut max_slots = 0;
	while index < functions.len() {
		let id = functions[index].id;
		let f = code.function(id)?;
		work.pay(f.len().checked_mul(f.slots as usize)?)?;
		if f.parameters == 0
			|| f.parameters > 4
			|| f.slots as usize > MAX_SLOTS
			|| f.len() == 0
			|| ops.len().checked_add(f.len())? > MAX_INSTRUCTIONS
			|| !f.cached_ref(0)?.charged
		{
			return None;
		}
		let start = ops.len();
		let args_base = args.len() as u32;
		for &a in &f.args {
			args.push(a).ok()?;
		}
		for pc in 0..f.len() {
			let mut op = f.cached_ref(pc)?.op;
			match op {
				Op::Call {
					out,
					function: INSTANCE_METHOD,
					args_start,
					args_len: 2,
				} => {
					let hash_slot = *f.args.get(args_start as usize)?;
					let self_slot = *f.args.get(args_start as usize + 1)?;
					let Op::Literal {
						out: literal_out,
						value: Value::U64(name),
					} = f.cached_ref(pc.checked_sub(1)?)?.op
					else {
						return None;
					};
					if literal_out != hash_slot {
						return None;
					}
					let target = code.region_script_method(Hash::associated_function(
						receiver,
						Hash::new(name),
					))?;
					let Op::CallValue {
						value,
						args_start: call_args,
						args_len: arity,
						..
					} = f.cached_ref(pc + 1)?.op
					else {
						return None;
					};
					if value != out
						|| f.args.get(call_args as usize) != Some(&self_slot)
						|| code.function(target)?.parameters != arity
					{
						return None;
					}
					if !sites
						.iter()
						.any(|s: &Site| s.name == name && s.target == target)
					{
						sites.push(Site { name, target }).ok()?;
					}
					op = Op::Literal {
						out,
						value: Value::Function {
							code: 0,
							id: target,
						},
					};
				}
				Op::CallValue {
					out,
					value,
					args_start,
					args_len,
				} => {
					let Op::Call {
						out: lookup_out,
						function: INSTANCE_METHOD,
						args_start: lookup_args,
						..
					} = f.cached_ref(pc.checked_sub(1)?)?.op
					else {
						return None;
					};
					if lookup_out != value {
						return None;
					}
					let hash_slot = *f.args.get(lookup_args as usize)?;
					let Op::Literal {
						out: literal_out,
						value: Value::U64(name),
					} = f.cached_ref(pc.checked_sub(2)?)?.op
					else {
						return None;
					};
					if literal_out != hash_slot {
						return None;
					}
					let function = code.region_script_method(Hash::associated_function(
						receiver,
						Hash::new(name),
					))?;
					op = Op::Call {
						out,
						function,
						args_start: args_base.checked_add(args_start)?,
						args_len,
					};
				}
				Op::Call {
					out,
					function,
					args_start,
					args_len,
				} => {
					code.function(function)?;
					op = Op::Call {
						out,
						function,
						args_start: args_base.checked_add(args_start)?,
						args_len,
					};
				}
				Op::Field { out, value, key }
				| Op::FieldGetMut { out, value, key }
				| Op::SetField {
					value,
					key,
					new: out,
				}
				| Op::FieldSetMut {
					value,
					key,
					new: out,
				} => {
					let mut found = None;
					for (i, &field) in fields.iter().enumerate() {
						if names_equal(code, field, id, key)? {
							found = Some(i);
							break;
						}
					}
					let field = match found {
						Some(i) => i,
						None => {
							if fields.len() >= MAX_FIELDS {
								return None;
							}
							fields
								.push(Field {
									function: id,
									key,
									kind: 0,
								})
								.ok()?;
							fields.len() - 1
						}
					} as u32;
					op = if matches!(op, Op::Field { .. } | Op::FieldGetMut { .. }) {
						Op::Field {
							out,
							value,
							key: field,
						}
					} else {
						Op::SetField {
							value,
							key: field,
							new: out,
						}
					};
				}
				Op::Jump { target } => {
					op = Op::Jump {
						target: (start as u32).checked_add(target)?,
					}
				}
				Op::Branch { condition, yes, no } => {
					op = Op::Branch {
						condition,
						yes: (start as u32).checked_add(yes)?,
						no: (start as u32).checked_add(no)?,
					}
				}
				Op::Literal {
					value: Value::I64(_) | Value::Bool(_) | Value::Unit | Value::U64(_),
					..
				}
				| Op::Copy { .. }
				| Op::Release { .. }
				| Op::Binary { .. }
				| Op::AssignBinary { .. }
				| Op::Get { .. }
				| Op::Set { .. }
				| Op::Return { .. } => (),
				_ => return None,
			}
			if let Op::Call { function, .. } = op {
				if !functions.iter().any(|f| f.id == function) {
					if functions.len() >= MAX_FUNCTIONS {
						return None;
					}
					functions
						.push(Function {
							id: function,
							start: 0,
							end: 0,
							slots: 0,
							parameters: 0,
						})
						.ok()?;
				}
			}
			ops.push(op).ok()?;
			charged.push(f.cached_ref(pc)?.charged).ok()?;
		}
		functions[index] = Function {
			id,
			start,
			end: ops.len(),
			slots: f.slots as usize,
			parameters: f.parameters as usize,
		};
		max_slots = max_slots.max(f.slots as usize);
		index += 1;
	}
	if fields.is_empty() {
		return None;
	}
	// Replace engine function identities by closed private entry labels.
	for op in ops.iter_mut() {
		match op {
			Op::Call { function, .. }
			| Op::Literal {
				value: Value::Function { id: function, .. },
				..
			} => {
				*function = functions.iter().find(|f| f.id == *function)?.start as u32;
			}
			_ => (),
		}
	}
	// A non-self cycle in the small closure is deferred, not recursively analyzed.
	let mut edges = [[false; MAX_FUNCTIONS]; MAX_FUNCTIONS];
	for (i, f) in functions.iter().enumerate() {
		for op in &ops[f.start..f.end] {
			if let Op::Call { function, .. } = op {
				let j = functions
					.iter()
					.position(|f| f.start == *function as usize)?;
				if i != j {
					edges[i][j] = true;
				}
			}
		}
	}
	for k in 0..functions.len() {
		for i in 0..functions.len() {
			for j in 0..functions.len() {
				edges[i][j] |= edges[i][k] && edges[k][j];
			}
		}
	}
	if (0..functions.len()).any(|i| edges[i][i]) {
		return None;
	}
	let mut states = BVec::new(budget);
	states
		.resize(ops.len().checked_mul(max_slots)?, 0u128)
		.ok()?;
	let mut reached = BVec::new(budget);
	reached.resize(ops.len(), false).ok()?;
	for f in functions.iter() {
		reached[f.start] = true;
		for slot in 0..max_slots {
			states[f.start * max_slots + slot] = if slot == 0 {
				SELF
			} else if slot < f.parameters {
				SCALAR
			} else {
				UNIT
			};
		}
	}
	let mut vector_tag = None;
	loop {
		let mut changed = false;
		for f in functions.iter() {
			for pc in f.start..f.end {
				work.pay(max_slots)?;
				if !reached[pc] {
					continue;
				}
				let mut state = [0u128; MAX_SLOTS];
				state[..max_slots].copy_from_slice(&states[pc * max_slots..(pc + 1) * max_slots]);
				let mut successors = [Some(pc + 1), None];
				let write = match ops[pc] {
					Op::Literal { out, value } => Some((
						out,
						match value {
							Value::I64(_) => INT,
							Value::Bool(_) => BOOL,
							Value::Unit => UNIT,
							Value::U64(_) => HASH,
							Value::Function { .. } => METHOD,
							_ => return None,
						},
					)),
					Op::Release { value } => Some((value, UNIT)),
					Op::Copy { out, value } => Some((out, state[value as usize])),
					Op::Field { out, value, key } => {
						if state[value as usize] != SELF {
							return None;
						}
						Some((out, 1 << (7 + key)))
					}
					Op::SetField { value, key, new } => {
						if state[value as usize] != SELF {
							return None;
						}
						kind(&mut fields, state[new as usize], 1)?;
						kind(&mut fields, 1 << (7 + key), 1)?;
						None
					}
					Op::Get { out, value, index } => {
						kind(&mut fields, state[value as usize], 2)?;
						kind(&mut fields, state[index as usize], 1)?;
						Some((out, ELEMENT))
					}
					Op::Set { value, index, new } => {
						kind(&mut fields, state[value as usize], 2)?;
						kind(&mut fields, state[index as usize], 1)?;
						let mask = state[new as usize];
						if mask == 0
							|| mask & !(INT | BOOL | ELEMENT) != 0
							|| mask & (INT | BOOL) == INT | BOOL
						{
							return None;
						}
						let tag = if mask & INT != 0 {
							Some(1)
						} else if mask & BOOL != 0 {
							Some(2)
						} else {
							None
						};
						if let Some(tag) = tag {
							if vector_tag.is_some_and(|t| t != tag) {
								return None;
							}
							vector_tag = Some(tag);
						}
						None
					}
					Op::Binary { out, op, lhs, rhs } | Op::AssignBinary { out, op, lhs, rhs } => {
						kind(&mut fields, state[lhs as usize], 1)?;
						kind(&mut fields, state[rhs as usize], 1)?;
						use crate::experimental_core::arithmetic::Binary::*;
						Some((
							out,
							match op {
								Add | Sub | Mul | Div | Rem => INT,
								Eq | Ne | Lt | Le | Gt | Ge => BOOL,
								_ => return None,
							},
						))
					}
					Op::Call {
						out,
						function,
						args_start,
						args_len,
					} => {
						let target = functions.iter().find(|f| f.start == function as usize)?;
						if target.parameters != args_len as usize {
							return None;
						}
						let parameters = args
							.get(args_start as usize..args_start.checked_add(args_len)? as usize)?;
						if state[*parameters.first()? as usize] != SELF {
							return None;
						}
						for &arg in &parameters[1..] {
							kind(&mut fields, state[arg as usize], 1)?;
						}
						Some((out, SCALAR))
					}
					Op::Branch { condition, yes, no } => {
						kind(&mut fields, state[condition as usize], 1)?;
						successors = [Some(yes as usize), Some(no as usize)];
						None
					}
					Op::Jump { target } => {
						successors = [Some(target as usize), None];
						None
					}
					Op::Return { value } => {
						kind(&mut fields, state[value as usize], 1)?;
						successors = [None, None];
						None
					}
					_ => return None,
				};
				if let Some((slot, mask)) = write {
					if slot == 0 || mask == 0 {
						return None;
					}
					state[slot as usize] = mask;
				}
				for next in successors.into_iter().flatten() {
					if next < f.start || next >= f.end {
						return None;
					}
					for (j, &value) in state.iter().enumerate().take(max_slots) {
						let at = next * max_slots + j;
						let joined = states[at] | value;
						changed |= states[at] != joined;
						states[at] = joined;
					}
					changed |= !reached[next];
					reached[next] = true;
				}
			}
		}
		if !changed {
			break;
		}
	}
	for field in fields.iter_mut() {
		if field.kind == 0 {
			field.kind = 1;
		}
	}
	let mut parameters = BVec::new(budget);
	for i in 0..root_fn.parameters {
		parameters.push(if i == 0 { 4u32 } else { 0 }).ok()?;
	}
	let mut types = BVec::new(budget);
	types.resize(ops.len().checked_mul(max_slots)?, 0u32).ok()?;
	let (ops, ob) = ops.into_parts();
	let (charged, cb) = charged.into_parts();
	let (args, ab) = args.into_parts();
	let (reached, rb) = reached.into_parts();
	let (parameters, pb) = parameters.into_parts();
	let (types, tb) = types.into_parts();
	let (functions, fb) = functions.into_parts();
	let (fields, db) = fields.into_parts();
	let (sites, sb) = sites.into_parts();
	let shape = Methods {
		code: code.id(),
		nominal,
		vector_tag,
		engine: Shape {
			function: root,
			slots: max_slots,
			parameters: parameters.into_std(),
			result: 0,
			ops: ops.into_std(),
			charged: charged.into_std(),
			args: args.into_std(),
			reached: reached.into_std(),
			types: types.into_std(),
		},
		functions: functions.into_std(),
		fields: fields.into_std(),
		sites: sites.into_std(),
	};
	shape
		.valid()
		.then_some((shape, ob + cb + ab + rb + pb + tb + fb + db + sb))
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::experimental_core::region::tests::main_of;
	use std::borrow::ToOwned;

	fn compile(source: &str) -> crate::experimental_core::frontend::Program {
		let context = crate::Context::with_default_modules().unwrap();
		let mut sources = crate::Sources::new();
		sources
			.insert(crate::Source::memory(source).unwrap())
			.unwrap();
		let mut diagnostics = crate::Diagnostics::new();
		crate::experimental_core::frontend::compile(&mut sources, &context, &mut diagnostics, 7)
			.unwrap_or_else(|error| panic!("{error:?}\n{:?}\n{source}", diagnostics.diagnostics()))
	}

	#[test]
	fn method_closure_closed_methods_admission_and_provenance_controls() {
		for (name, source) in [
			(
				"fields",
				include_str!("../fixtures/method_fields.rn").to_owned(),
			),
			(
				"vector",
				include_str!("../fixtures/method_vector.rn").to_owned(),
			),
			(
				"permute",
				include_str!(concat!(
					env!("CARGO_MANIFEST_DIR"),
					"/../../benches/awfy/permute.rn"
				))
				.to_owned(),
			),
		] {
			let source = source
				+ if name == "permute" {
					include_str!(concat!(
						env!("CARGO_MANIFEST_DIR"),
						"/../../benches/awfy/benchmark.rn"
					))
				} else {
					""
				};
			let source = source
				+ if name == "permute" {
					"\npub fn main(seed) {new_benchmark().benchmark()}"
				} else {
					"\npub fn main(seed) {kernel(seed)}"
				};
			let program = compile(&source);
			let budget = Budget::new(8 << 20);
			let mut count = 0;
			for id in 0..program.code.functions() as u32 {
				let mut work = 1_000_000;
				if let Some((shape, bytes)) = analyze(&program.code, id, &budget, &mut work) {
					assert_eq!(shape.code, program.code.id());
					assert!(shape.valid());
					assert!(shape.metadata() <= bytes);
					assert!(shape.function(0).is_some());
					assert!(!shape.fields.is_empty());
					count += usize::from(shape.functions.len() > 1);
					drop(shape);
					budget.release(bytes);
				}
				assert_eq!(budget.used(), 0);
			}
			assert!(count > 0, "no multi-function {name} closure");
		}
		let polymorphic = compile("struct A{x} struct B{x} impl A{fn tick(self){self.x}} impl B{fn tick(self){self.x+1}} fn apply(x){x.tick()} pub fn main(n){apply(A{x:1})+apply(B{x:2})}");
		let mut work = 1_000_000;
		assert!(analyze(
			&polymorphic.code,
			polymorphic.function(Hash::type_hash(["apply"])).unwrap(),
			&Budget::new(8 << 20),
			&mut work
		)
		.is_none());
		for source in [
			"struct A{x} impl A{fn run(self,n){self.x=[];}} pub fn main(n){A{x:0}.run(n)}",
			"struct A{x} impl A{fn run(self,n){self.other(n)} fn other(self,n){self.run(n)}} pub fn main(n){A{x:0}.run(n)}",
		] {
			let p=compile(source);
			let budget=Budget::new(8 << 20);
			for id in 0..p.code.functions() as u32 {
				let mut work=1_000_000;
				assert!(analyze(&p.code,id,&budget,&mut work).is_none());
				assert_eq!(budget.used(),0);
			}
			let mut zero=0;
			assert!(analyze(&p.code,main_of(&p),&budget,&mut zero).is_none());
		}
	}

	#[test]
	#[cfg(all(target_arch = "x86_64", target_os = "linux"))]
	#[cfg_attr(
		miri,
		ignore = "actual executable code; safe entry/publication has separate coverage"
	)]
	fn method_closure_shared_compiler_private_permute_activations() {
		let source = std::format!(
			"{}\n{}\npub fn main(n) {{new_benchmark().benchmark()}}",
			include_str!(concat!(
				env!("CARGO_MANIFEST_DIR"),
				"/../../benches/awfy/permute.rn"
			)),
			include_str!(concat!(
				env!("CARGO_MANIFEST_DIR"),
				"/../../benches/awfy/benchmark.rn"
			))
		);
		let p = compile(&source);
		let mut work = 1_000_000;
		let budget = Budget::new(8 << 20);
		let nominal = (0..p.code.functions() as u32)
			.find_map(|id| {
				let f = p.code.function(id)?;
				(0..f.len()).find_map(|pc| match f.operation(pc).ok()? {
					Op::Record {
						tag: Type::Struct(n),
						..
					} => Some(n),
					_ => None,
				})
			})
			.unwrap();
		let target = p
			.code
			.region_script_method(Hash::associated_function(
				Hash::new(nominal::hash_copy(nominal).unwrap()),
				Hash::ident("permute"),
			))
			.unwrap();
		let (s, bytes) = analyze(&p.code, target, &budget, &mut work).unwrap();
		let compiled =
			crate::experimental_native::lower::recursion::compile_methods(&s, 0).unwrap();
		let native =
			crate::experimental_native::boundary::NativeRecursion::load(&compiled).unwrap();
		let depth = 16;
		let frames = super::super::activation_cells(s.engine.slots, depth).unwrap();
		let values = frames + HEADER_CELLS;
		let tags = values + s.fields.len();
		let vector = tags + s.fields.len();
		let mut scratch = std::vec![0i64;vector+6];
		for i in 0..s.engine.slots {
			scratch[s.engine.slots + i] = 3;
		}
		scratch[s.engine.slots] = 4;
		scratch[1] = 6;
		scratch[s.engine.slots + 1] = 1;
		scratch[frames + ROOM_SLOTS] = 160;
		scratch[frames + VECTOR_LEN] = 6;
		scratch[frames + VECTOR_TAG] = 1;
		for (i, f) in s.fields.iter().enumerate() {
			scratch[tags + i] = if f.kind == 2 { 5 } else { 1 };
			scratch[values + i] = if f.kind == 2 { 1 } else { 0 };
		}
		for i in 0..6 {
			scratch[vector + i] = i as i64 + 10;
		}
		let mut control = [depth as i64];
		let mut record = super::super::Record::new(1_000_000);
		assert_eq!(
			native.call(&mut control, &mut record, &mut scratch),
			Some(1)
		);
		assert_eq!(record.calls, 18_737);
		assert_eq!(record.peak, 6);
		assert_eq!(scratch[frames + HELPER_CALLS], 10_078);
		assert_eq!(scratch[frames + SELF_CALLS], 8_659);
		for (i, f) in s.fields.iter().enumerate() {
			if f.kind == 1 {
				assert_eq!(scratch[values + i], 8_660);
			}
		}
		assert_eq!(&scratch[vector..], &[10, 11, 12, 13, 14, 15]);
		drop(s);
		budget.release(bytes);
		assert_eq!(budget.used(), 0);
	}

	#[test]
	#[cfg(all(target_arch = "x86_64", target_os = "linux"))]
	#[cfg_attr(
		miri,
		ignore = "executor's actual native code; safe transaction has separate Miri coverage"
	)]
	fn method_closure_executor_cold_warm_every_fuel_and_nested_fault() {
		use crate::experimental_core::{
			execute::{take_remaining_work_for_test, Executor},
			heap::{Limits, Store},
			region::cache,
		};
		for source in [
			std::format!("{}\npub fn main(n) {{kernel(n)}}",include_str!("../fixtures/method_fields.rn").replace("200 + seed % 7", "3 + seed % 3")),
			std::format!("{}\npub fn main(n) {{kernel(n)}}",include_str!("../fixtures/method_vector.rn").replace("100 + seed % 7", "3 + seed % 3")),
			std::string::String::from("struct Pair{total,values} impl Pair{fn mutate(self,n){self.total=self.total+1;self.values[0]=self.values[0]+2;self.read(n)} fn read(self,n){self.values[n]}} pub fn main(n){Pair{total:3,values:[1,2]}.mutate(n)}"),
			std::string::String::from("struct Pair{total,values} impl Pair{fn mutate(self,n){self.values[0]=self.values[0]+2;self.total=self.total+1;self.total+9223372036854775807}} pub fn main(n){Pair{total:3,values:[1,2]}.mutate(n)}"),
			std::string::String::from("struct Pair{total,values} impl Pair{fn mutate(self,n){self.values[0]=self.values[0]+2;self.total=self.total+1;}} pub fn main(n){Pair{total:false,values:[1,2]}.mutate(n)}"),
		] {
			let p=compile(&source);cache::clear_for_test();cache::enable_for_test(true);cache::prepare(&p.code).unwrap();
			assert!(!cache::methods::counters(p.code.id()).is_empty(),"no loaded method closure: {:?}",cache::summary(p.code.id()));
			for fuel in 0..=450 {
				let mut observations=std::vec::Vec::new();
				for native in [false,true] {
					let mut store=Store::new(Limits::default()).unwrap();let arg=store.root_i64(5);let mut e=Executor::new().unwrap();e.set_native_enabled(native);
					// Small fixtures complete within this fuel sweep; warm physical
					// engine room before testing every stopping point.
					if let Ok(value)=e.invoke_code(&p.code,&mut store,main_of(&p),&[&arg],1_000_000){store.root_release(value).unwrap();}
					let r=e.invoke_code(&p.code,&mut store,main_of(&p),&[&arg],fuel);
					let result=match r{Ok(value)=>{let answer=store.root_scalar(&value).unwrap();store.root_release(value).unwrap();Ok(answer)},Err(f)=>Err((f.error,f.call_chain,f.cleanup_error,f.remaining_work))};
					observations.push((result,take_remaining_work_for_test(),e.capacities_for_test(),store.state_for_test()));
					assert!(e.is_idle());assert_eq!(store.live_entries(),0);store.root_release(arg).unwrap();
				}
				assert_eq!(observations[0],observations[1],"fuel{fuel} {source}");
				if fuel==450 && source.contains("fn kernel") {assert!(observations[0].0.is_ok(),"small fixture did not complete inside fuel sweep");}
			}
			let counters=cache::methods::counters(p.code.id());assert!(counters.iter().any(|r|r.1>0));
			assert!(counters.iter().map(|r|r.1).sum::<u64>()<=2*451,"descendant retries escaped root suppression");
			if source.contains("fn kernel") {assert!(counters.iter().any(|r|r.2>0&&r.4>0));}
			else {assert!(counters.iter().any(|r|r.3>0));}
			cache::clear_for_test();cache::enable_for_test(false);
		}
	}

	#[test]
	#[cfg(all(target_arch = "x86_64", target_os = "linux"))]
	#[cfg_attr(
		miri,
		ignore = "actual loaded native programs; pure publication checked separately"
	)]
	fn method_closure_shared_aliases_distinct_vectors_and_cache_lifetime() {
		use crate::experimental_core::{
			execute::Executor,
			heap::{Limits, Store},
			region::cache,
		};
		let p=compile("fn fact(n){if n<=1{1}else{n*fact(n-1)}} fn scan(v){let sum=0;for x in v{sum+=x;}sum} fn plain(n){let sum=0;for i in 0..n{sum+=i;}sum} fn indexed(v){for i in 0..2{v[i]=v[i]+1;}v[0]+v[1]} struct Bag{left,right,total} impl Bag{fn update(self){self.total+=1;self.left[0]=self.left[0]+1;self.right[0]=self.right[0]+10;self.total}} pub fn main(n){let v=[1];let w=if n==0{v}else{[3]};let bag=Bag{left:v,right:w,total:0};let alias=bag;bag.update();alias.total*100+v[0]*10+w[0]+fact(3)-6+scan([1,2])-3+plain(3)-3+indexed([1,2])-5}");
		cache::clear_for_test();
		cache::enable_for_test(true);
		cache::prepare(&p.code).unwrap();
		for (input, expected) in [(0, 232), (1, 133)] {
			let before: u64 = cache::methods::counters(p.code.id())
				.iter()
				.map(|r| r.1)
				.sum();
			let mut store = Store::new(Limits::default()).unwrap();
			let arg = store.root_i64(input);
			let mut e = Executor::new().unwrap();
			let result = e
				.invoke_code(&p.code, &mut store, main_of(&p), &[&arg], 1000)
				.unwrap();
			assert_eq!(
				store.root_scalar(&result).unwrap(),
				Some(crate::experimental_core::value::Scalar::I64(expected))
			);
			store.root_release(result).unwrap();
			store.root_release(arg).unwrap();
			assert_eq!(store.live_entries(), 0);
			assert!(e.is_idle());
			let after: u64 = cache::methods::counters(p.code.id())
				.iter()
				.map(|r| r.1)
				.sum();
			assert_eq!(
				after - before,
				u64::from(input == 0),
				"distinct Vecs refuse at entry; shared aliases share one buffer"
			);
		}
		assert!(cache::recursive_counters_for_test(p.code.id())
			.iter()
			.any(|r| r.0 > 0));
		let guard = cache::checkout_for_test(&p.code).unwrap();
		assert!(guard.methods().unwrap().loops.has_collections());
		let address = core::ptr::from_ref(guard.program()) as usize;
		assert!(cache::checkout_for_test(&p.code).is_none());
		drop(guard);
		let unwind = std::panic::catch_unwind(|| {
			let guard = cache::checkout_for_test(&p.code).unwrap();
			assert_eq!(core::ptr::from_ref(guard.program()) as usize, address);
			std::panic!("unwind checked-out mixed descriptor");
		});
		assert!(unwind.is_err());
		assert!(cache::resident_for_test(p.code.id()));
		let guard = cache::checkout_for_test(&p.code).unwrap();
		assert_eq!(core::ptr::from_ref(guard.program()) as usize, address);
		cache::clear_for_test();
		assert!(guard.methods().is_some());
		assert!(!cache::resident_for_test(p.code.id()));
		assert_eq!(cache::cache_state_for_test(), (0, 0, 0, 0));
		drop(guard);
		assert!(cache::prepare(&p.code).unwrap().loaded > 0);
		let restored = cache::checkout_for_test(&p.code).unwrap();
		assert!(restored.methods().is_some());
		drop(restored);
		cache::clear_for_test();
		cache::enable_for_test(false);
	}

	#[test]
	#[cfg(all(target_arch = "x86_64", target_os = "linux"))]
	#[cfg_attr(
		miri,
		ignore = "actual native frame/fault replay; safe preparation has Miri controls"
	)]
	fn method_closure_frame_slot_and_allocation_limits_match_interpreter() {
		use crate::experimental_core::{
			execute::{take_remaining_work_for_test, Executor, ExecutorLimits},
			heap::{Limits, Store},
			region::cache,
		};
		let source = std::format!(
			"{}\npub fn main(n){{kernel(n)}}",
			include_str!("../fixtures/method_vector.rn")
		);
		let p = compile(&source);
		cache::clear_for_test();
		cache::enable_for_test(true);
		cache::prepare(&p.code).unwrap();
		for limits in [
			ExecutorLimits::default(),
			ExecutorLimits {
				frames: 4,
				slots: 64,
			},
			ExecutorLimits {
				frames: 64,
				slots: 32,
			},
		] {
			for quota in (0..=4096).step_by(64) {
				let mut observations = std::vec::Vec::new();
				for native in [false, true] {
					let mut store = Store::new(Limits::default()).unwrap();
					let arg = store.root_i64(0);
					let mut e = Executor::with_limits(limits).unwrap();
					e.set_native_enabled(native);
					if let Ok(value) =
						e.invoke_code(&p.code, &mut store, main_of(&p), &[&arg], 10_000)
					{
						store.root_release(value).unwrap();
					}
					let r = rune_alloc::limit::with(quota, || {
						(e.invoke_code(&p.code, &mut store, main_of(&p), &[&arg], 10_000),)
					})
					.call()
					.0;
					let result = match r {
						Ok(value) => {
							let result = store.root_scalar(&value).unwrap();
							store.root_release(value).unwrap();
							Ok(result)
						}
						Err(f) => Err((f.error, f.call_chain, f.remaining_work, f.cleanup_error)),
					};
					observations.push((
						result,
						take_remaining_work_for_test(),
						e.capacities_for_test(),
						store.state_for_test(),
					));
					assert!(e.is_idle());
					assert_eq!(store.live_entries(), 0);
					store.root_release(arg).unwrap();
				}
				assert_eq!(
					observations[0], observations[1],
					"quota{quota} limits{limits:?}"
				);
			}
		}
		cache::clear_for_test();
		cache::enable_for_test(false);
	}

	#[test]
	#[cfg(all(target_arch = "x86_64", target_os = "linux"))]
	#[cfg_attr(
		miri,
		ignore = "native mapping installation; no unsafe transaction in this test"
	)]
	fn method_closure_descriptor_allocation_failure_is_cached_and_invisible() {
		use crate::experimental_core::{
			execute::Executor,
			heap::{Limits, Store},
			region::cache,
		};
		let p = compile("struct Tally{sum} fn bump(p){p.sum+1} pub fn main(n){bump(Tally{sum:n})}");
		cache::clear_for_test();
		cache::enable_for_test(true);
		cache::fail_next_descriptor_for_test();
		let summary = cache::prepare(&p.code).unwrap();
		assert_eq!(summary.regions, 1);
		assert_eq!(summary.loaded, 0);
		assert_eq!(summary.metadata, 1);
		assert_eq!(cache::cache_state_for_test(), (1, 0, 0, 0));
		assert!(matches!(
			cache::checkout(&p.code),
			cache::Checkout::Interpret { no_regions: false }
		));
		assert!(cache::checkout_for_test(&p.code).is_none());
		let mut store = Store::new(Limits::default()).unwrap();
		let arg = store.root_i64(3);
		let mut e = Executor::new().unwrap();
		for _ in 0..3 {
			let result = e
				.invoke_code(&p.code, &mut store, main_of(&p), &[&arg], 100)
				.unwrap();
			assert_eq!(
				store.root_scalar(&result).unwrap(),
				Some(crate::experimental_core::value::Scalar::I64(4))
			);
			store.root_release(result).unwrap();
			assert!(e.is_idle());
			assert_eq!(store.live_entries(), 0);
		}
		store.root_release(arg).unwrap();
		assert_eq!(cache::summary(p.code.id()).unwrap(), summary);
		assert_eq!(cache::cache_state_for_test(), (1, 0, 0, 0));
		cache::clear_for_test();
		cache::enable_for_test(false);
	}
}
