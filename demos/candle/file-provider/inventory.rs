//! Normalize the stock installer's typed rows to the versioned C declarations.
//! Unknown signatures must fail here instead of silently losing a callable.
pub(super) fn write(rows: Vec<String>, header: bool) -> Result<(), Box<dyn std::error::Error>> {
	for row in rows {
		let fields: Vec<_> = row.split('\t').collect();
		let [path, shape, recv, arg, result, error] = fields[..] else {
			return Err("bad stock inventory row".into());
		};
		let path = path.strip_prefix("::").ok_or("missing crate name")?;
		if !path
			.split("::")
			.all(|s| !s.is_empty() && s.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_'))
		{
			return Err(format!("unrepresentable name {path}").into());
		}
		let kind = |ty: &str| {
			Ok::<_, String>(match ty {
				"" => 0,
				"::polars::DataFrame" => 1,
				"::interchange::Dense" => 2,
				"::candle::Tensor" => 3,
				"::candle::Mlp" => 4,
				"::polars::Error" => 5,
				_ => return Err(format!("unknown native type {ty}")),
			})
		};
		let (recv, arg, result, error) = (kind(recv)?, kind(arg)?, kind(result)?, kind(error)?);
		let code = match shape {
			"csv" => 1,
			"columns" => 2,
			"convert" => 3,
			"load" => 4,
			"forward" => 5,
			"unary" => 6,
			"integer" => 7,
			"pair" => 8,
			"triple" => 9,
			"text" => 10,
			"names" => 11,
			"display" => 12,
			"panic-csv" => 13,
			"session-factory" => 14,
			"session-result" if error == 5 => 15,
			"resolution" => 16,
			"session-error-text" => 17,
			_ => return Err(format!("unmapped stock shape {shape} at {path}").into()),
		};
		if header {
			if code >= 14 {
				println!("REFUSAL(\"{path}\",{code},{recv},{arg},{result}),");
			}
		} else {
			println!("{path}\t{code}\t{recv}\t{arg}\t{result}");
		}
	}
	Ok(())
}
