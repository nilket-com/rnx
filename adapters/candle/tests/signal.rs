//! Record 0145: Candle's signal operations. Each result equals Candle's own
//! on the LOGICAL inputs (made contiguous first), for plain tensors and for
//! strided, transposed and offset views of inputs and kernels; refusals by
//! name; the receiver and kernel stay the script's.
use candle_core::{DType, Device, Tensor as CTensor};
use rnx::rune::{self, Context, Module, Source, Sources, Value, Vm};
use std::sync::Arc;

fn eval(body: &str) -> Result<Value, String> {
	let mut candle = Module::with_crate("candle").unwrap();
	rnx_candle::build(&mut candle).unwrap();
	let mut context = Context::with_default_modules().unwrap();
	context
		.install(rnx::interchange::module().unwrap())
		.unwrap();
	context.install(candle).unwrap();
	let runtime = Arc::new(context.runtime().unwrap());
	let mut sources = Sources::new();
	let script = format!(
		"fn seq(shape, dt) {{ let n = 1; for d in shape {{ n *= d; }} candle::Tensor::arange(0, n, \"f64\")?.affine(0.37, -1.5)?.sin()?.to_dtype(dt)?.reshape(shape) }}\n\
		 pub fn main() {{ {body} }}"
	);
	sources.insert(Source::memory(script).unwrap()).unwrap();
	let mut diagnostics = rune::Diagnostics::new();
	let unit = rune::prepare(&mut sources)
		.with_context(&context)
		.with_diagnostics(&mut diagnostics)
		.build();
	let unit = unit.unwrap_or_else(|_| panic!("{:?}", diagnostics.diagnostics()));
	let mut vm = Vm::new(runtime, Arc::new(unit));
	let v = vm.call(["main"], ()).map_err(|e| e.to_string())?;
	match rune::from_value::<Result<Value, Value>>(v.clone()) {
		Ok(Ok(v)) => Ok(v),
		Ok(Err(e)) => Err(rune::from_value::<String>(e).unwrap_or_else(|_| "?".into())),
		Err(_) => Ok(v),
	}
}

fn refused(body: &str, want: &str) {
	match eval(body) {
		Ok(_) => panic!("accepted: {body}"),
		Err(e) => assert!(e.contains(want), "{body}\n  got: {e}\n  want: {want}"),
	}
}

/// The same deterministic values as the script's `seq`.
fn seq(shape: &[usize], dt: DType) -> CTensor {
	let n: usize = shape.iter().product();
	CTensor::arange(0f64, n as f64, &Device::Cpu)
		.unwrap()
		.affine(0.37, -1.5)
		.unwrap()
		.sin()
		.unwrap()
		.to_dtype(dt)
		.unwrap()
		.reshape(shape)
		.unwrap()
}

fn bits(t: &CTensor) -> (Vec<usize>, Vec<u8>) {
	let mut v = Vec::new();
	t.write_bytes(&mut v).unwrap();
	(t.dims().to_vec(), v)
}

fn ours(v: &Value) -> (Vec<usize>, Vec<u8>) {
	let t = v.borrow_ref::<rnx_candle::Tensor>().unwrap();
	bits(rnx_candle::tensor_ops::inner(&t))
}

fn c(t: &CTensor) -> CTensor {
	t.force_contiguous().unwrap()
}

#[test]
fn convolutions_match_candle_on_logical_inputs() {
	for (dt, name) in [(DType::F32, "f32"), (DType::F64, "f64")] {
		// conv1d: padding, stride, dilation, groups
		for (p, s, d, g) in [(0, 1, 1, 1), (2, 2, 1, 1), (1, 1, 2, 2), (3, 3, 2, 4)] {
			let x = seq(&[2, 4, 17], dt);
			let k = seq(&[8, 4 / g, 3], dt);
			let want = c(&x).conv1d(&c(&k), p, s, d, g).unwrap();
			let got = eval(&format!(
				"seq([2, 4, 17], {name:?})?.conv1d(seq([8, {}, 3], {name:?})?, {p}, {s}, {d}, {g})",
				4 / g
			))
			.unwrap();
			assert_eq!(ours(&got), bits(&want), "conv1d {name} {p} {s} {d} {g}");
			let algo = eval(&format!(
				"seq([2, 4, 17], {name:?})?.conv1d_with_algo(seq([8, {}, 3], {name:?})?, {p}, {s}, {d}, {g}, \"winograd\")",
				4 / g
			))
			.unwrap();
			assert_eq!(ours(&algo), bits(&want), "conv1d_with_algo {name}");
		}
		// conv2d: the three upstream branches (1x1 fast, 1x1 full im2col, tiled)
		for (k, p, s, d, g) in [
			(1, 0, 1, 1, 1),
			(1, 1, 2, 1, 2),
			(3, 1, 1, 1, 1),
			(3, 2, 2, 2, 2),
		] {
			let x = seq(&[2, 4, 9, 11], dt);
			let kt = seq(&[6, 4 / g, k, k], dt);
			let want = c(&x).conv2d(&c(&kt), p, s, d, g).unwrap();
			let got = eval(&format!(
				"seq([2, 4, 9, 11], {name:?})?.conv2d(seq([6, {}, {k}, {k}], {name:?})?, {p}, {s}, {d}, {g})",
				4 / g
			))
			.unwrap();
			assert_eq!(
				ours(&got),
				bits(&want),
				"conv2d {name} k{k} {p} {s} {d} {g}"
			);
			let algo = eval(&format!(
				"seq([2, 4, 9, 11], {name:?})?.conv2d_with_algo(seq([6, {}, {k}, {k}], {name:?})?, {p}, {s}, {d}, {g}, \"none\")",
				4 / g
			))
			.unwrap();
			assert_eq!(ours(&algo), bits(&want), "conv2d_with_algo {name}");
		}
		// conv_transpose1d: the col2im path and the direct path; groups
		for (p, op, s, d, g) in [
			(0, 0, 1, 1, 1),
			(0, 0, 2, 1, 2),
			(1, 1, 2, 1, 1),
			(2, 0, 3, 2, 2),
		] {
			let x = seq(&[2, 4, 7], dt);
			let k = seq(&[4, 3, 3], dt);
			let want = c(&x).conv_transpose1d(&c(&k), p, op, s, d, g).unwrap();
			let got = eval(&format!(
				"seq([2, 4, 7], {name:?})?.conv_transpose1d(seq([4, 3, 3], {name:?})?, {p}, {op}, {s}, {d}, {g})"
			))
			.unwrap();
			assert_eq!(
				ours(&got),
				bits(&want),
				"conv_transpose1d {name} {p} {op} {s} {d} {g}"
			);
		}
		for (p, op, s, d) in [(0, 0, 1, 1), (1, 1, 2, 1), (1, 0, 2, 2)] {
			let x = seq(&[2, 3, 5, 6], dt);
			let k = seq(&[3, 2, 3, 3], dt);
			let want = c(&x).conv_transpose2d(&c(&k), p, op, s, d).unwrap();
			let got = eval(&format!(
				"seq([2, 3, 5, 6], {name:?})?.conv_transpose2d(seq([3, 2, 3, 3], {name:?})?, {p}, {op}, {s}, {d})"
			))
			.unwrap();
			assert_eq!(ours(&got), bits(&want), "conv_transpose2d {name}");
		}
	}
}

/// Review round (plan) R3: strided, transposed and offset views of inputs
/// and kernels give the logical result, for every signal operation. The
/// upstream defect is shown directly: Candle's own conv1d on a transposed
/// kernel differs from the logical result.
#[test]
fn views_give_the_logical_result_and_the_upstream_defect_is_shown() {
	let dt = DType::F32;
	// a transposed kernel: [3, 4, 8] permuted to [8, 4, 3]
	let raw = seq(&[3, 4, 8], dt);
	let kv = raw.permute((2, 1, 0)).unwrap();
	assert!(!kv.is_contiguous());
	let x = seq(&[2, 4, 17], dt);
	let logical = c(&x).conv1d(&c(&kv), 1, 1, 1, 1).unwrap();
	let upstream = x.conv1d(&kv, 1, 1, 1, 1).unwrap();
	assert_ne!(bits(&upstream), bits(&logical), "the upstream defect");
	let got = eval(
		"seq([2, 4, 17], \"f32\")?.conv1d(seq([3, 4, 8], \"f32\")?.permute([2, 1, 0])?, 1, 1, 1, 1)",
	)
	.unwrap();
	assert_eq!(ours(&got), bits(&logical), "conv1d, a transposed kernel");
	// an offset input (narrowed) and a strided input (transposed back)
	let xo = seq(&[3, 4, 17], dt).narrow(0, 1, 2).unwrap();
	let want = c(&xo).conv1d(&c(&kv), 1, 1, 1, 1).unwrap();
	let got = eval(
		"seq([3, 4, 17], \"f32\")?.narrow(0, 1, 2)?.conv1d(seq([3, 4, 8], \"f32\")?.permute([2, 1, 0])?, 1, 1, 1, 1)",
	)
	.unwrap();
	assert_eq!(ours(&got), bits(&want), "conv1d, an offset input");
	// conv2d with a transposed kernel and a transposed input
	let k2 = seq(&[3, 3, 4, 6], dt).permute((3, 2, 0, 1)).unwrap();
	let x2 = seq(&[2, 4, 11, 9], dt).transpose(2, 3).unwrap();
	let want = c(&x2).conv2d(&c(&k2), 1, 1, 1, 1).unwrap();
	let got = eval(
		"seq([2, 4, 11, 9], \"f32\")?.transpose(2, 3)?.conv2d(seq([3, 3, 4, 6], \"f32\")?.permute([3, 2, 0, 1])?, 1, 1, 1, 1)",
	)
	.unwrap();
	assert_eq!(ours(&got), bits(&want), "conv2d views");
	// transposed convolutions with views
	let kt = seq(&[3, 3, 4], dt).permute((2, 1, 0)).unwrap();
	let want = c(&x.narrow(1, 0, 4).unwrap())
		.conv_transpose1d(&c(&kt), 0, 0, 1, 1, 1)
		.unwrap();
	let got = eval(
		"seq([2, 4, 17], \"f32\")?.narrow(1, 0, 4)?.conv_transpose1d(seq([3, 3, 4], \"f32\")?.permute([2, 1, 0])?, 0, 0, 1, 1, 1)",
	)
	.unwrap();
	assert_eq!(ours(&got), bits(&want), "conv_transpose1d views");
	let kt2 = seq(&[2, 3, 3, 3], dt).permute((1, 0, 2, 3)).unwrap();
	let xt2 = seq(&[2, 3, 6, 5], dt).transpose(2, 3).unwrap();
	let want = c(&xt2).conv_transpose2d(&c(&kt2), 1, 1, 2, 1).unwrap();
	let got = eval(
		"seq([2, 3, 6, 5], \"f32\")?.transpose(2, 3)?.conv_transpose2d(seq([2, 3, 3, 3], \"f32\")?.permute([1, 0, 2, 3])?, 1, 1, 2, 1)",
	)
	.unwrap();
	assert_eq!(ours(&got), bits(&want), "conv_transpose2d views");
	// pooling and resampling on transposed and offset inputs
	let pv = seq(&[2, 3, 9, 8], dt).transpose(2, 3).unwrap();
	let script = "seq([2, 3, 9, 8], \"f32\")?.transpose(2, 3)?";
	for (call, want) in [
		("avg_pool2d([2, 3])", c(&pv).avg_pool2d((2, 3)).unwrap()),
		("max_pool2d([3, 2])", c(&pv).max_pool2d((3, 2)).unwrap()),
		(
			"avg_pool2d_with_stride([3, 3], [2, 1])",
			c(&pv).avg_pool2d_with_stride((3, 3), (2, 1)).unwrap(),
		),
		(
			"max_pool2d_with_stride([2, 2], [1, 3])",
			c(&pv).max_pool2d_with_stride((2, 2), (1, 3)).unwrap(),
		),
		(
			"upsample_nearest2d(13, 5)",
			c(&pv).upsample_nearest2d(13, 5).unwrap(),
		),
		("interpolate2d(4, 17)", c(&pv).interpolate2d(4, 17).unwrap()),
		(
			"upsample_bilinear2d(13, 5, false)",
			c(&pv).upsample_bilinear2d(13, 5, false).unwrap(),
		),
		(
			"upsample_bilinear2d(13, 5, true)",
			c(&pv).upsample_bilinear2d(13, 5, true).unwrap(),
		),
		(
			"upsample_bilinear2d_with_scale(1.5, 0.5, false)",
			c(&pv)
				.upsample_bilinear2d_with_scale(1.5, 0.5, false)
				.unwrap(),
		),
		(
			"upsample_bilinear2d_with_scale(2.0, 3.0, true)",
			c(&pv)
				.upsample_bilinear2d_with_scale(2.0, 3.0, true)
				.unwrap(),
		),
	] {
		let got = eval(&format!("{script}.{call}")).unwrap();
		assert_eq!(ours(&got), bits(&want), "{call}");
	}
	let lv = seq(&[3, 2, 9], dt).narrow(0, 1, 2).unwrap();
	for (call, want) in [
		(
			"upsample_nearest1d(20)",
			c(&lv).upsample_nearest1d(20).unwrap(),
		),
		("interpolate1d(4)", c(&lv).interpolate1d(4).unwrap()),
	] {
		let got = eval(&format!("seq([3, 2, 9], \"f32\")?.narrow(0, 1, 2)?.{call}")).unwrap();
		assert_eq!(ours(&got), bits(&want), "{call}");
	}
}

#[test]
fn every_size_and_argument_is_refused_before_the_call() {
	let x1 = "seq([1, 2, 8], \"f32\")?";
	let k1 = "seq([2, 2, 3], \"f32\")?";
	for (call, want) in [
		(format!("{x1}.conv1d({k1}, 0, 0, 1, 1)"), "stride must be at least 1"),
		(format!("{x1}.conv1d({k1}, 0, 1, 0, 1)"), "dilation must be at least 1"),
		(format!("{x1}.conv1d({k1}, 0, 1, 1, 3)"), "groups 3 must divide"),
		(format!("{x1}.conv1d({k1}, 0, 1, 1, 0)"), "groups must be at least 1"),
		(format!("{x1}.conv1d({k1}, 0, 1, 1, 5000)"), "at most 4096"),
		(format!("{x1}.conv1d(seq([2, 3, 3], \"f32\")?, 0, 1, 1, 1)"), "the kernel takes 3 per group"),
		(format!("{x1}.conv1d(seq([2, 2, 9], \"f32\")?, 0, 1, 1, 1)"), "exceeds the padded input"),
		(format!("{x1}.conv1d({k1}, 0, 1, 5, 1)"), "exceeds the padded input"),
		(format!("{x1}.conv1d({k1}, -1, 1, 1, 1)"), "padding must be at least 0"),
		(format!("{x1}.conv1d(seq([2, 2, 3], \"f64\")?, 0, 1, 1, 1)"), "dtypes differ"),
		(format!("{x1}.conv1d({k1}, 0, 1, 1, 1, 7)"), "arguments"),
		(format!("{x1}.conv1d_with_algo({k1}, 0, 1, 1, 1, \"fastest\")"), "algo \"fastest\""),
		(format!("candle::Tensor::zeros([1, 2, 0], \"f32\")?.conv1d({k1}, 0, 1, 1, 1)"), "empty axis"),
		("candle::Tensor::zeros([1, 2, 8], \"i64\")?.conv1d(candle::Tensor::zeros([2, 2, 3], \"i64\")?, 0, 1, 1, 1)".into(), "needs f32 or f64"),
		(format!("seq([2, 8], \"f32\")?.conv1d({k1}, 0, 1, 1, 1)"), "must be rank 3"),
		(format!("{x1}.conv_transpose1d(seq([2, 2, 3], \"f32\")?, 0, 1, 1, 1, 1)"), "output_padding 1 must be below"),
		// the pinned restriction: (l - 1)·s < 2p underflows upstream
		("seq([1, 1, 1], \"f32\")?.conv_transpose1d(seq([1, 1, 3], \"f32\")?, 1, 0, 1, 1, 1)".into(), "a pinned restriction"),
		(format!("{x1}.conv_transpose1d(seq([3, 2, 3], \"f32\")?, 0, 0, 1, 1, 1)"), "the kernel 3"),
		("seq([1, 1, 2, 2], \"f32\")?.conv_transpose2d(seq([1, 1, 1, 1], \"f32\")?, 2, 0, 1, 1)".into(), "leaves no output"),
		("seq([1, 1, 4, 4], \"f32\")?.avg_pool2d([0, 2])".into(), "the kernel size must be"),
		("seq([1, 1, 4, 4], \"f32\")?.max_pool2d([5, 2])".into(), "larger than the input"),
		("seq([1, 1, 4, 4], \"f32\")?.avg_pool2d_with_stride([2, 2], [0, 1])".into(), "the stride must be"),
		("seq([1, 1, 4, 4], \"f32\")?.upsample_nearest2d(0, 3)".into(), "must be at least 1"),
		("seq([1, 1, 4], \"f32\")?.upsample_nearest1d(16777217)".into(), "at most 16777216"),
		("candle::Tensor::zeros([1, 1, 0], \"f32\")?.interpolate1d(3)".into(), "empty axis"),
		("seq([1, 1, 4, 4], \"f32\")?.upsample_bilinear2d_with_scale(0.0 / 0.0, 1.0, false)".into(), "finite and positive"),
		("seq([1, 1, 4, 4], \"f32\")?.upsample_bilinear2d_with_scale(1.0 / 0.0, 1.0, false)".into(), "finite and positive"),
		("seq([1, 1, 4, 4], \"f32\")?.upsample_bilinear2d_with_scale(-1.0, 1.0, false)".into(), "finite and positive"),
		("seq([1, 1, 4, 4], \"f32\")?.upsample_bilinear2d_with_scale(0.1, 1.0, false)".into(), "want 1 to"),
		("seq([1, 1, 4, 4], \"f32\")?.upsample_bilinear2d_with_scale(10000000.0, 1.0, false)".into(), "want 1 to"),
		("seq([1, 1, 4, 4], \"f32\")?.upsample_bilinear2d_with_scale(2000.0, 2000.0, false)".into(), "at most 16777216"),
		("seq([1, 1, 4, 4], \"u32\")?.upsample_bilinear2d(5, 5, false)".into(), "needs f32 or f64"),
	] {
		refused(&call, want);
	}
}

#[test]
fn the_receiver_and_kernel_stay_the_scripts() {
	let v = eval(
		"let x = seq([1, 2, 8], \"f32\")?; let k = seq([2, 2, 3], \"f32\")?; \
		 let ok = x.conv1d(k, 1, 1, 1, 1)?; let bad = x.conv1d(k, 0, 0, 1, 1); \
		 let t = x.conv_transpose1d(seq([2, 2, 3], \"f32\")?, 0, 0, 1, 1, 1)?; \
		 Ok((ok.dims(), bad.is_err(), x.dims(), k.dims(), t.dims(), x.max_pool2d([1, 1]).is_err()))",
	)
	.unwrap();
	let got: (Vec<i64>, bool, Vec<i64>, Vec<i64>, Vec<i64>, bool) = rune::from_value(v).unwrap();
	assert_eq!(
		got,
		(
			vec![1, 2, 8],
			true,
			vec![1, 2, 8],
			vec![2, 2, 3],
			vec![1, 2, 10],
			true
		)
	);
}
