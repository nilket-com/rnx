//! Separately sealed N5 artifact. No native calls, relocations or unbounded stack
//! growth: self calls are branches with private, physically checked continuations.
use super::*;
use crate::experimental_core::region::recursion::methods::{self, Methods};
use crate::experimental_core::region::recursion::{tag, Shape};

pub(crate) struct CompiledRecursion(Compiled);
impl CompiledRecursion {
	pub(crate) fn code_bytes(&self) -> usize {
		self.0.bytes().len()
	}

	pub(in crate::experimental_native) fn artifact(&self) -> &Compiled {
		&self.0
	}
}

/// This is the only constructor. Both IR audits and the unchanged machine gate
/// run before the artifact can be loaded; no arbitrary IR can enter this API.
pub(crate) fn compile(
	shape: &Shape,
	program_bytes: usize,
) -> Result<CompiledRecursion, NativeRefusal> {
	compile_shared::<false>(shape, None, program_bytes)
}

pub(crate) fn compile_methods(
	shape: &Methods,
	program_bytes: usize,
) -> Result<CompiledRecursion, NativeRefusal> {
	compile_shared::<true>(&shape.engine, Some(shape), program_bytes)
}

fn compile_shared<const METHODS: bool>(
	shape: &Shape,
	methods: Option<&Methods>,
	program_bytes: usize,
) -> Result<CompiledRecursion, NativeRefusal> {
	if if METHODS {
		!methods.is_some_and(Methods::valid)
	} else {
		!shape.valid_layout()
	} {
		return Err(NativeRefusal::Shape);
	}
	let sites = shape
		.ops
		.iter()
		.enumerate()
		.filter(|(pc, op)| {
			shape.reached(*pc) && matches!(op, crate::experimental_core::code::Op::Call { .. })
		})
		.count();
	// Conservative bound on generated blocks, checked BEFORE Cranelift allocates
	// any IR. Reuse the existing native node budget, never a call-depth limit.
	let blocks = shape
		.ops
		.len()
		.saturating_mul(if METHODS { 16 } else { 4 })
		.saturating_add((sites + 3).saturating_mul(2 * shape.slots + 3));
	if blocks > crate::experimental_core::region::MAX_REGION_NODES {
		return Err(NativeRefusal::Shape);
	}
	let isa = shared_isa()?;
	let func = build::<METHODS>(shape, methods, &isa)?;
	cranelift_codegen::verify_function(&func, &*isa)
		.map_err(|e| NativeRefusal::Verifier(e.to_string()))?;
	let map = super::super::audit::AccessMap::recursion();
	super::super::audit::ir(&func, &map).map_err(NativeRefusal::Audit)?;
	#[cfg(test)]
	let clif = func.display().to_string();
	let mut ctx = Context::for_function(func);
	ctx.set_disasm(true);
	ctx.compile(&*isa, &mut ControlPlane::default())
		.map_err(|e| NativeRefusal::Codegen(format!("{:?}", e.inner)))?;
	let mut code = ctx.take_compiled_code().ok_or(NativeRefusal::Shape)?;
	let bytes = code.code_buffer().to_vec();
	let alignment = u64::from(code.buffer.alignment);
	let vcode = code.vcode.take().unwrap_or_default();
	let report = super::super::audit::machine(&code, &vcode).map_err(NativeRefusal::Audit)?;

	let optimized_audit = super::super::audit::ir(&ctx.func, &map);
	#[cfg(test)]
	if optimized_audit.is_err() {
		std::println!("N5 optimized refusal IR:\n{}", ctx.func.display());
	}
	optimized_audit.map_err(NativeRefusal::Audit)?;

	if bytes.len() > super::super::MAX_REGION_CODE_BYTES
		|| program_bytes.saturating_add(bytes.len()) > super::super::MAX_PROGRAM_CODE_BYTES
	{
		return Err(NativeRefusal::Size);
	}
	#[cfg(test)]
	let listing = Listing {
		clif,
		optimized: ctx.func.display().to_string(),
		vcode,
		report,
	};
	#[cfg(not(test))]
	drop((vcode, report));
	Ok(CompiledRecursion(Compiled {
		collection: true,
		bytes,
		alignment,
		cells: 1,
		#[cfg(test)]
		listing,
	}))
}

struct Memory {
	base: ir::Value,
	len: ir::Value,
	fail: Block,
}
impl Memory {
	/// Always clamp the FULL flattened index before scaling, not just frame base.
	fn address(&self, b: &mut FunctionBuilder<'_>, index: ir::Value) -> ir::Value {
		let ok = b.ins().icmp(IntCC::UnsignedLessThan, index, self.len);
		let good = b.create_block();
		b.ins().brif(ok, good, &[], self.fail, &[]);
		b.switch_to_block(good);
		let zero = b.ins().iconst(types::I64, 0);
		let safe = b.ins().select_spectre_guard(ok, index, zero);
		let scaled = b.ins().ishl_imm_s(safe, 3);
		b.ins().iadd(self.base, scaled)
	}
	fn index(
		&self,
		b: &mut FunctionBuilder<'_>,
		depth: ir::Value,
		stride: usize,
		slot: usize,
	) -> ir::Value {
		let base = b.ins().imul_imm_s(depth, stride as i64);
		b.ins().iadd_imm_s(base, slot as i64)
	}
	fn load(
		&self,
		b: &mut FunctionBuilder<'_>,
		depth: ir::Value,
		stride: usize,
		slot: usize,
	) -> ir::Value {
		let index = self.index(b, depth, stride, slot);
		let address = self.address(b, index);
		b.ins()
			.load(types::I64, MemFlagsData::trusted(), address, 0)
	}
	fn store(
		&self,
		b: &mut FunctionBuilder<'_>,
		depth: ir::Value,
		stride: usize,
		slot: usize,
		value: ir::Value,
	) {
		let index = self.index(b, depth, stride, slot);
		let address = self.address(b, index);
		b.ins().store(MemFlagsData::trusted(), value, address, 0);
	}
}

fn build<const METHODS: bool>(
	s: &Shape,
	methods: Option<&Methods>,
	isa: &OwnedTargetIsa,
) -> Result<ir::Function, NativeRefusal> {
	use crate::experimental_core::code::Op;
	let mut func = ir::Function::with_name_signature(
		UserFuncName::user(1, s.function),
		collection_signature(),
	);
	let mut fctx = FunctionBuilderContext::new();
	let mut b = FunctionBuilder::new(&mut func, &mut fctx);
	let entry = b.create_block();
	b.append_block_params_for_function_params(entry);
	let pcs: Vec<_> = s.ops.iter().map(|_| b.create_block()).collect();
	let replay = b.create_block();
	b.set_cold_block(replay);
	let success = b.create_block();
	b.append_block_param(success, types::I64);
	let ret = b.create_block();
	b.append_block_param(ret, types::I64);
	if METHODS {
		b.append_block_param(ret, types::I64);
	}
	let occupied = METHODS.then(|| b.declare_var(types::I64));
	let vals: Vec<_> = (0..2 * s.slots)
		.map(|_| b.declare_var(types::I64))
		.collect();
	let fuel = b.declare_var(types::I64);
	let depth = b.declare_var(types::I64);
	let calls = b.declare_var(types::I64);
	let peak = b.declare_var(types::I64);
	let work = b.declare_var(types::I64);
	let stride = 2 * s.slots + 1;
	b.switch_to_block(entry);
	let args = b.block_params(entry);
	let control = args[0];
	let record = args[1];
	let memory = Memory {
		base: args[2],
		len: args[3],
		fail: replay,
	};
	let maxdepth = b
		.ins()
		.load(types::I64, MemFlagsData::trusted(), control, 0);
	let initial = b.ins().load(
		types::I64,
		MemFlagsData::trusted(),
		record,
		ExitRecord::FUEL,
	);
	let zero = b.ins().iconst(types::I64, 0);
	b.def_var(fuel, initial);
	b.def_var(depth, zero);
	b.def_var(calls, zero);
	b.def_var(peak, zero);
	b.def_var(work, zero);
	if let Some(occupied) = occupied {
		b.def_var(occupied, zero);
	}
	let layout = if METHODS {
		let shape = methods.ok_or(NativeRefusal::Shape)?;
		let count = b.ins().iadd_imm_s(maxdepth, 1);
		let frames = b.ins().imul_imm_s(count, stride as i64);
		let field_values = b.ins().iadd_imm_s(frames, methods::HEADER_CELLS as i64);
		let field_tags = b.ins().iadd_imm_s(field_values, shape.fields.len() as i64);
		let vector = b.ins().iadd_imm_s(field_tags, shape.fields.len() as i64);
		Some(HeapMemory {
			header: frames,
			field_values,
			field_tags,
			vector,
		})
	} else {
		None
	};
	for (j, &var) in vals.iter().enumerate() {
		let v = memory.load(&mut b, zero, stride, j);
		b.def_var(var, v);
	}
	b.ins().jump(pcs[0], &[]);
	let mut sites = Vec::new();
	for (pc, op) in s.ops.iter().enumerate() {
		b.switch_to_block(pcs[pc]);
		// A zero type on slot0 need not imply unreachability (a dead mixed slot).
		// A separate forward reachability test is supplied by analysis below.
		if !s.reached(pc) {
			b.ins().jump(replay, &[]);
			continue;
		}
		if matches!(op, Op::Return { .. }) {
			let d = b.use_var(depth);
			let root = b.ins().icmp_imm_s(IntCC::Equal, d, 0);
			let done = b.ins().iconst(types::I64, pc as i64);
			let nested = b.create_block();
			b.ins().brif(root, success, &[done.into()], nested, &[]);
			b.switch_to_block(nested);
		}
		let w = b.use_var(work);
		let w = b.ins().iadd_imm_s(w, 1);
		b.def_var(work, w);
		if s.charged[pc] {
			let f = b.use_var(fuel);
			let empty = b.ins().icmp_imm_s(IntCC::Equal, f, 0);
			let paid = b.create_block();
			b.ins().brif(empty, replay, &[], paid, &[]);
			b.switch_to_block(paid);
			let f = b.ins().iadd_imm_s(f, -1);
			b.def_var(fuel, f);
		}
		let mut next = Some(pc + 1);
		match *op {
			Op::Literal { out, value } => {
				let (t, v) = if METHODS {
					match value {
						crate::experimental_core::value::Value::U64(n) => (6, n as i64),
						crate::experimental_core::value::Value::Function { code: 0, id } => {
							(7, id as i64)
						}
						_ => tag(value).ok_or(NativeRefusal::Shape)?,
					}
				} else {
					tag(value).ok_or(NativeRefusal::Shape)?
				};
				let v = b.ins().iconst(types::I64, v);
				let t = b.ins().iconst(types::I64, t as i64);
				b.def_var(vals[out as usize], v);
				b.def_var(vals[s.slots + out as usize], t);
			}
			Op::Release { value } => {
				let t = b.ins().iconst(types::I64, 3);
				b.def_var(vals[value as usize], zero);
				b.def_var(vals[s.slots + value as usize], t);
			}
			Op::Copy { out, value } => {
				let v = b.use_var(vals[value as usize]);
				let t = b.use_var(vals[s.slots + value as usize]);
				b.def_var(vals[out as usize], v);
				b.def_var(vals[s.slots + out as usize], t);
			}
			Op::Binary { out, op, lhs, rhs } | Op::AssignBinary { out, op, lhs, rhs } => {
				if METHODS {
					require_tag(&mut b, vals[s.slots + lhs as usize], 1, replay);
					require_tag(&mut b, vals[s.slots + rhs as usize], 1, replay);
				}
				let lhs = b.use_var(vals[lhs as usize]);
				let rhs = b.use_var(vals[rhs as usize]);
				let (v, t) = match op {
					Binary::Add | Binary::Sub | Binary::Mul => {
						let (v, overflow) = match op {
							Binary::Add => b.ins().sadd_overflow(lhs, rhs),
							Binary::Sub => b.ins().ssub_overflow(lhs, rhs),
							_ => b.ins().smul_overflow(lhs, rhs),
						};
						let good = b.create_block();
						b.ins().brif(overflow, replay, &[], good, &[]);
						b.switch_to_block(good);
						(v, 1)
					}
					Binary::Div | Binary::Rem => {
						let z = b.ins().icmp_imm_s(IntCC::Equal, rhs, 0);
						let min = b.ins().icmp_imm_s(IntCC::Equal, lhs, i64::MIN);
						let neg = b.ins().icmp_imm_s(IntCC::Equal, rhs, -1);
						let ov = b.ins().band(min, neg);
						let bad = b.ins().bor(z, ov);
						let good = b.create_block();
						b.ins().brif(bad, replay, &[], good, &[]);
						b.switch_to_block(good);
						(
							if op == Binary::Div {
								b.ins().sdiv(lhs, rhs)
							} else {
								b.ins().srem(lhs, rhs)
							},
							1,
						)
					}
					_ => {
						let cc = match op {
							Binary::Eq => IntCC::Equal,
							Binary::Ne => IntCC::NotEqual,
							Binary::Lt => IntCC::SignedLessThan,
							Binary::Le => IntCC::SignedLessThanOrEqual,
							Binary::Gt => IntCC::SignedGreaterThan,
							Binary::Ge => IntCC::SignedGreaterThanOrEqual,
							_ => return Err(NativeRefusal::Shape),
						};
						let v = b.ins().icmp(cc, lhs, rhs);
						(b.ins().uextend(types::I64, v), 2)
					}
				};
				b.def_var(vals[out as usize], v);
				let t = b.ins().iconst(types::I64, t);
				b.def_var(vals[s.slots + out as usize], t);
			}
			Op::Branch { condition, yes, no } => {
				if METHODS {
					require_tag(&mut b, vals[s.slots + condition as usize], 2, replay);
				}
				let v = b.use_var(vals[condition as usize]);
				b.ins()
					.brif(v, pcs[yes as usize], &[], pcs[no as usize], &[]);
				next = None;
			}
			Op::Jump { target } => {
				next = Some(target as usize);
			}
			Op::Call {
				out,
				args_start,
				args_len,
				function,
			} => {
				let target = if METHODS {
					methods
						.and_then(|m| m.function(function as usize))
						.ok_or(NativeRefusal::Shape)?
						.slots
				} else {
					s.slots
				};
				let d = b.use_var(depth);
				let full = b.ins().icmp(IntCC::UnsignedGreaterThanOrEqual, d, maxdepth);
				let good = b.create_block();
				b.ins().brif(full, replay, &[], good, &[]);
				b.switch_to_block(good);
				if let Some(occupied) = occupied {
					let old = b.use_var(occupied);
					let next = b.ins().iadd_imm_s(old, target as i64);
					let layout = layout.as_ref().ok_or(NativeRefusal::Shape)?;
					let room = memory.load_offset(&mut b, layout.header, methods::ROOM_SLOTS);
					let fits = b.ins().icmp(IntCC::UnsignedLessThanOrEqual, next, room);
					let good = b.create_block();
					b.ins().brif(fits, good, &[], replay, &[]);
					b.switch_to_block(good);
					b.def_var(occupied, next);
					let owner = methods
						.ok_or(NativeRefusal::Shape)?
						.functions
						.iter()
						.find(|f| f.start <= pc && pc < f.end)
						.ok_or(NativeRefusal::Shape)?;
					let counter = if owner.start == function as usize {
						methods::SELF_CALLS
					} else {
						methods::HELPER_CALLS
					};
					let old = memory.load_offset(&mut b, layout.header, counter);
					let next = b.ins().iadd_imm_s(old, 1);
					memory.store_offset(&mut b, layout.header, counter, next);
				}
				for (j, &var) in vals.iter().enumerate() {
					let v = b.use_var(var);
					memory.store(&mut b, d, stride, j, v);
				}
				let site = b.ins().iconst(types::I64, sites.len() as i64);
				memory.store(&mut b, d, stride, 2 * s.slots, site);
				sites.push((out, pc + 1, target));
				let mut parameters = Vec::new();
				let mut parameter_tags = Vec::new();
				for &slot in &s.args[args_start as usize..(args_start + args_len) as usize] {
					parameters.push(b.use_var(vals[slot as usize]));
					if METHODS {
						parameter_tags.push(b.use_var(vals[s.slots + slot as usize]));
					}
				}
				for j in 0..s.slots {
					let v = parameters.get(j).copied().unwrap_or(zero);
					b.def_var(vals[j], v);
					let t = if METHODS {
						match parameter_tags.get(j) {
							Some(&t) => t,
							None => b.ins().iconst(types::I64, 3),
						}
					} else {
						b.ins()
							.iconst(types::I64, s.parameters.get(j).copied().unwrap_or(3) as i64)
					};
					b.def_var(vals[s.slots + j], t);
				}
				let d = b.ins().iadd_imm_s(d, 1);
				b.def_var(depth, d);
				let p = b.use_var(peak);
				let p = b.ins().umax(p, d);
				b.def_var(peak, p);
				let c = b.use_var(calls);
				let c = b.ins().iadd_imm_s(c, 1);
				b.def_var(calls, c);
				next = Some(if METHODS { function as usize } else { 0 });
			}
			Op::Field { out, key, .. } if METHODS => {
				let layout = layout.as_ref().ok_or(NativeRefusal::Shape)?;
				let v = memory.load_offset(&mut b, layout.field_values, key as usize);
				let t = memory.load_offset(&mut b, layout.field_tags, key as usize);
				b.def_var(vals[out as usize], v);
				b.def_var(vals[s.slots + out as usize], t);
			}
			Op::SetField { key, new, .. } if METHODS => {
				let layout = layout.as_ref().ok_or(NativeRefusal::Shape)?;
				let v = b.use_var(vals[new as usize]);
				let t = b.use_var(vals[s.slots + new as usize]);
				memory.store_offset(&mut b, layout.field_values, key as usize, v);
				memory.store_offset(&mut b, layout.field_tags, key as usize, t);
			}
			Op::Get { out, index, .. } if METHODS => {
				require_tag(&mut b, vals[s.slots + index as usize], 1, replay);
				let layout = layout.as_ref().ok_or(NativeRefusal::Shape)?;
				let index = b.use_var(vals[index as usize]);
				let address = layout.element(&mut b, &memory, index);
				let v = b
					.ins()
					.load(types::I64, MemFlagsData::trusted(), address, 0);
				let t = memory.load_offset(&mut b, layout.header, methods::VECTOR_TAG);
				b.def_var(vals[out as usize], v);
				b.def_var(vals[s.slots + out as usize], t);
			}
			Op::Set { index, new, .. } if METHODS => {
				require_tag(&mut b, vals[s.slots + index as usize], 1, replay);
				let layout = layout.as_ref().ok_or(NativeRefusal::Shape)?;
				let index = b.use_var(vals[index as usize]);
				let v = b.use_var(vals[new as usize]);
				let address = layout.element(&mut b, &memory, index);
				b.ins().store(MemFlagsData::trusted(), v, address, 0);
			}
			Op::Return { value } => {
				let v = b.use_var(vals[value as usize]);
				if METHODS {
					let t = b.use_var(vals[s.slots + value as usize]);
					b.ins().jump(ret, &[v.into(), t.into()]);
				} else {
					b.ins().jump(ret, &[v.into()]);
				}
				next = None;
			}
			_ => return Err(NativeRefusal::Shape),
		}
		if let Some(next) = next {
			b.ins().jump(pcs[next], &[]);
		}
	}
	b.switch_to_block(ret);
	let returned = b.block_params(ret)[0];
	let returned_tag = METHODS.then(|| b.block_params(ret)[1]);
	let d = b.use_var(depth);
	let d = b.ins().iadd_imm_s(d, -1);
	b.def_var(depth, d);
	let site = memory.load(&mut b, d, stride, 2 * s.slots);
	for (j, &var) in vals.iter().enumerate() {
		let v = memory.load(&mut b, d, stride, j);
		b.def_var(var, v);
	}
	for (i, &(out, next, callee_slots)) in sites.iter().enumerate() {
		let hit = b.ins().icmp_imm_s(IntCC::Equal, site, i as i64);
		let found = b.create_block();
		let other = b.create_block();
		b.ins().brif(hit, found, &[], other, &[]);
		b.switch_to_block(found);
		b.def_var(vals[out as usize], returned);
		let t = match returned_tag {
			Some(t) => t,
			None => b.ins().iconst(types::I64, s.result as i64),
		};
		b.def_var(vals[s.slots + out as usize], t);
		if let Some(occupied) = occupied {
			let old = b.use_var(occupied);
			let next = b.ins().iadd_imm_s(old, -(callee_slots as i64));
			b.def_var(occupied, next);
		}
		b.ins().jump(pcs[next], &[]);
		b.switch_to_block(other);
	}
	b.ins().jump(replay, &[]);
	b.switch_to_block(success);
	let pc = b.block_params(success)[0];
	for (j, &var) in vals.iter().enumerate() {
		let v = b.use_var(var);
		memory.store(&mut b, zero, stride, j, v);
	}
	let one = b.ins().iconst(types::I32, 1);
	b.ins()
		.store(MemFlagsData::trusted(), one, record, ExitRecord::STATUS);
	let pc = b.ins().ireduce(types::I32, pc);
	b.ins()
		.store(MemFlagsData::trusted(), pc, record, ExitRecord::PC);
	let f = b.use_var(fuel);
	b.ins()
		.store(MemFlagsData::trusted(), f, record, ExitRecord::FUEL);
	let c = b.use_var(calls);
	b.ins()
		.store(MemFlagsData::trusted(), c, record, ExitRecord::WRITTEN);
	let p = b.use_var(peak);
	b.ins()
		.store(MemFlagsData::trusted(), p, record, ExitRecord::UNITS);
	let w = b.use_var(work);
	b.ins()
		.store(MemFlagsData::trusted(), w, record, ExitRecord::BOOLS);
	b.ins().return_(&[one]);
	b.switch_to_block(replay);
	let status = b.ins().iconst(types::I32, 2);
	b.ins()
		.store(MemFlagsData::trusted(), status, record, ExitRecord::STATUS);
	let w = b.use_var(work);
	b.ins()
		.store(MemFlagsData::trusted(), w, record, ExitRecord::BOOLS);
	b.ins().return_(&[status]);
	b.seal_all_blocks();
	b.finalize(isa.frontend_config());
	Ok(func)
}

struct HeapMemory {
	header: ir::Value,
	field_values: ir::Value,
	field_tags: ir::Value,
	vector: ir::Value,
}
impl Memory {
	fn load_offset(
		&self,
		b: &mut FunctionBuilder<'_>,
		base: ir::Value,
		offset: usize,
	) -> ir::Value {
		let index = b.ins().iadd_imm_s(base, offset as i64);
		let address = self.address(b, index);
		b.ins()
			.load(types::I64, MemFlagsData::trusted(), address, 0)
	}
	fn store_offset(
		&self,
		b: &mut FunctionBuilder<'_>,
		base: ir::Value,
		offset: usize,
		value: ir::Value,
	) {
		let index = b.ins().iadd_imm_s(base, offset as i64);
		let address = self.address(b, index);
		b.ins().store(MemFlagsData::trusted(), value, address, 0);
	}
}
impl HeapMemory {
	fn element(&self, b: &mut FunctionBuilder<'_>, memory: &Memory, index: ir::Value) -> ir::Value {
		let len = memory.load_offset(b, self.header, methods::VECTOR_LEN);
		let ok = b.ins().icmp(IntCC::UnsignedLessThan, index, len);
		let good = b.create_block();
		b.ins().brif(ok, good, &[], memory.fail, &[]);
		b.switch_to_block(good);
		let zero = b.ins().iconst(types::I64, 0);
		let safe = b.ins().select_spectre_guard(ok, index, zero);
		let flat = b.ins().iadd(self.vector, safe);
		memory.address(b, flat)
	}
}
fn require_tag(b: &mut FunctionBuilder<'_>, tag: Variable, expected: i64, replay: Block) {
	let t = b.use_var(tag);
	let ok = b.ins().icmp_imm_s(IntCC::Equal, t, expected);
	let good = b.create_block();
	b.ins().brif(ok, good, &[], replay, &[]);
	b.switch_to_block(good);
}
