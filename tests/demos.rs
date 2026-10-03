//! Record 0155: the terminal showcase's demos. Each demo runs from the
//! repository root with the built `rnx`; its stdout must equal the committed
//! transcript byte for byte, with exit 0 and an empty stderr. Then every
//! answer in the transcript is parsed into labelled fields and checked
//! against an independent Rust computation from the bundled data or from
//! first principles; a value found under the wrong label fails (plans/0155
//! section 4a, item 3). The controls feed swapped or renamed transcripts to
//! the same checkers and require a rejection.
use std::collections::BTreeMap;
use std::path::Path;
use std::process::Command;

fn root() -> &'static Path {
	Path::new(env!("CARGO_MANIFEST_DIR"))
}

/// Runs a demo as documented: `rnx run demos/NAME.rn` from the root.
fn run(name: &str) -> String {
	let out = Command::new(env!("CARGO_BIN_EXE_rnx"))
		.args(["run", &format!("demos/{name}.rn")])
		.current_dir(root())
		.output()
		.unwrap();
	assert!(out.status.success(), "{name}: exit {:?}", out.status);
	assert!(
		out.stderr.is_empty(),
		"{name}: stderr {}",
		String::from_utf8_lossy(&out.stderr)
	);
	let stdout = String::from_utf8(out.stdout).unwrap();
	let want = std::fs::read_to_string(root().join(format!("demos/out/{name}.txt"))).unwrap();
	assert_eq!(
		stdout, want,
		"{name}: the output differs from its transcript"
	);
	stdout
}

/// Cents as `d.cc`, the order demos' format.
fn dollars(cents: i64) -> String {
	format!("{}.{:02}", cents / 100, cents % 100)
}

/// f64 money as the mortgage demo shows it: rounded cents, thousands separators.
fn money(x: f64) -> String {
	let cents = (x * 100.0).round() as i64;
	let whole = (cents / 100).to_string();
	let mut out = String::new();
	for (i, ch) in whole.chars().enumerate() {
		if i > 0 && (whole.len() - i) % 3 == 0 {
			out.push(',');
		}
		out.push(ch);
	}
	format!("{out}.{:02}", cents % 100)
}

/// The value after `label` on the one line that starts with it (whitespace-normalized).
fn field(text: &str, label: &str) -> Result<String, String> {
	let hits: Vec<&str> = text
		.lines()
		.filter(|l| l.trim_start().starts_with(label))
		.collect();
	match hits.as_slice() {
		[line] => Ok(line.trim_start()[label.len()..].trim().to_owned()),
		_ => Err(format!("{} lines start with {label:?}", hits.len())),
	}
}

fn check_mortgage(text: &str) -> Result<(), String> {
	let (p, r, n) = (300000.0_f64, 0.06_f64 / 12.0, 360);
	let pay = p * r / (1.0 - (1.0 + r).powi(-n));
	let mut want = BTreeMap::new();
	want.insert("Monthly payment".to_owned(), money(pay));
	want.insert("Total paid".to_owned(), money(pay * 360.0));
	want.insert("Total interest".to_owned(), money(pay * 360.0 - p));
	let mut balance = p;
	for month in 1..=360 {
		balance -= pay - balance * r;
		if month % 12 == 0 && [1, 5, 10, 20, 30].contains(&(month / 12)) {
			let shown = if balance.abs() < 0.005 { 0.0 } else { balance };
			want.insert(format!("year {:>2}", month / 12), money(shown));
		}
	}
	for (label, value) in &want {
		let got = field(text, label)?;
		if &got != value {
			return Err(format!("{label}: transcript {got}, computed {value}"));
		}
	}
	// the hand-computed anchor: a 300,000 loan at 6% over 30 years pays 1,798.65 a month
	if field(text, "Monthly payment")? != "1,798.65" {
		return Err("the payment is not the known 1,798.65".into());
	}
	Ok(())
}

struct Order {
	customer: String,
	region: String,
	qty: i64,
	cents: i64,
	status: String,
}

fn orders() -> Vec<Order> {
	let v: serde_json::Value = serde_json::from_str(
		&std::fs::read_to_string(root().join("demos/data/orders.json")).unwrap(),
	)
	.unwrap();
	v.as_array()
		.unwrap()
		.iter()
		.map(|o| Order {
			customer: o["customer"].as_str().unwrap().to_owned(),
			region: o["region"].as_str().unwrap().to_owned(),
			qty: o["qty"].as_i64().unwrap(),
			cents: o["unit_cents"].as_i64().unwrap(),
			status: o["status"].as_str().unwrap().to_owned(),
		})
		.collect()
}

/// Shipped revenue in cents per key (shipped only, plans/0155 4a item 2).
fn revenue_by(os: &[Order], key: impl Fn(&Order) -> &str) -> BTreeMap<String, i64> {
	let mut m = BTreeMap::new();
	for o in os {
		let c = if o.status == "shipped" {
			o.qty * o.cents
		} else {
			0
		};
		*m.entry(key(o).to_owned()).or_insert(0) += c;
	}
	m
}

fn check_orders(text: &str) -> Result<(), String> {
	let os = orders();
	let regions = revenue_by(&os, |o| &o.region);
	let section: Vec<&str> = text
		.lines()
		.skip_while(|l| !l.starts_with("Shipped revenue by region"))
		.skip(1)
		.take_while(|l| l.starts_with("  "))
		.collect();
	let mut got = BTreeMap::new();
	for line in &section {
		let f: Vec<&str> = line.split_whitespace().collect();
		if f.len() != 2 {
			return Err(format!("region line {line:?}"));
		}
		got.insert(f[0].to_owned(), f[1].to_owned());
	}
	let want: BTreeMap<String, String> = regions
		.iter()
		.map(|(k, v)| (k.clone(), dollars(*v)))
		.collect();
	if got != want {
		return Err(format!("regions: transcript {got:?}, computed {want:?}"));
	}
	let mut ranked: Vec<(i64, String)> = revenue_by(&os, |o| &o.customer)
		.into_iter()
		.map(|(k, v)| (-v, k))
		.collect();
	ranked.sort();
	let (a, b) = (&ranked[0], &ranked[1]);
	let want = format!(
		"{} ({}), ahead of {} ({})",
		a.1,
		dollars(-a.0),
		b.1,
		dollars(-b.0)
	);
	let got = field(text, "Largest customer:")?;
	if got != want {
		return Err(format!(
			"largest customer: transcript {got:?}, computed {want:?}"
		));
	}
	let open = os.iter().filter(|o| o.status == "open").count();
	let want = format!(
		"{open} of {} orders ({:.1}%)",
		os.len(),
		open as f64 * 100.0 / os.len() as f64
	);
	let got = field(text, "Still open:")?;
	if got != want {
		return Err(format!("open share: transcript {got:?}, computed {want:?}"));
	}
	Ok(())
}

fn check_report(text: &str) -> Result<(), String> {
	let os = orders();
	let rev = revenue_by(&os, |o| &o.region);
	let total: i64 = rev.values().sum();
	let mut want = BTreeMap::new();
	for region in rev.keys() {
		let n = os.iter().filter(|o| &o.region == region).count();
		let units: i64 = os
			.iter()
			.filter(|o| &o.region == region && o.status == "shipped")
			.map(|o| o.qty)
			.sum();
		let share = format!("{:.1}%", rev[region] as f64 * 100.0 / total as f64);
		want.insert(
			region.clone(),
			vec![
				n.to_string(),
				units.to_string(),
				dollars(rev[region]),
				share,
			],
		);
	}
	let units: i64 = os
		.iter()
		.filter(|o| o.status == "shipped")
		.map(|o| o.qty)
		.sum();
	want.insert(
		"total".into(),
		vec![
			os.len().to_string(),
			units.to_string(),
			dollars(total),
			"100.0%".into(),
		],
	);
	let mut got = BTreeMap::new();
	for line in text.lines().skip(2) {
		let f: Vec<&str> = line.split_whitespace().collect();
		if f.len() >= 5 && f[0].chars().all(|c| c.is_ascii_lowercase()) {
			got.insert(
				f[0].to_owned(),
				f[1..5].iter().map(|s| s.to_string()).collect::<Vec<_>>(),
			);
		}
	}
	if got != want {
		return Err(format!(
			"report rows: transcript {got:?}, computed {want:?}"
		));
	}
	Ok(())
}

#[test]
fn mortgage_transcript_and_answers() {
	check_mortgage(&run("01_mortgage")).unwrap();
}

#[test]
fn orders_transcript_and_answers() {
	check_orders(&run("02_orders")).unwrap();
}

#[test]
fn report_transcript_and_answers() {
	check_report(&run("03_report")).unwrap();
}

/// The checkers bind values to labels: a swapped row or a renamed label fails.
#[test]
fn checkers_reject_swapped_or_renamed_values() {
	let read =
		|n: &str| std::fs::read_to_string(root().join(format!("demos/out/{n}.txt"))).unwrap();
	let orders = read("02_orders");
	let swapped = orders
		.replace("north    245.00", "north    144.97")
		.replacen("east     144.97", "east     245.00", 1);
	assert_ne!(swapped, orders);
	assert!(
		check_orders(&swapped).is_err(),
		"two regions' revenue swapped was accepted"
	);
	let renamed = orders.replace("Acme (134.99)", "Birch (134.99)");
	assert_ne!(renamed, orders);
	assert!(
		check_orders(&renamed).is_err(),
		"a renamed largest customer was accepted"
	);
	let report = read("03_report");
	let swapped = report.replace(
		"east          3     11    144.97",
		"east          3     11    245.00",
	);
	assert_ne!(swapped, report);
	assert!(
		check_report(&swapped).is_err(),
		"a report row with another region's revenue was accepted"
	);
	let mortgage = read("01_mortgage");
	let swapped = mortgage.replace("year  5    279,163.07", "year  5    251,057.17");
	assert_ne!(swapped, mortgage);
	assert!(
		check_mortgage(&swapped).is_err(),
		"a balance under the wrong year was accepted"
	);
}

/// The copy shows real output: each ```text block in demos/README.md, in
/// order, is a verbatim slice of the matching transcript.
#[test]
fn readme_excerpts_are_transcript_slices() {
	let readme = std::fs::read_to_string(root().join("demos/README.md")).unwrap();
	let blocks: Vec<&str> = readme
		.split("```text\n")
		.skip(1)
		.map(|b| b.split("```").next().unwrap())
		.collect();
	let names = ["01_mortgage", "02_orders", "03_report"];
	assert_eq!(blocks.len(), names.len());
	for (block, name) in blocks.iter().zip(names) {
		let transcript =
			std::fs::read_to_string(root().join(format!("demos/out/{name}.txt"))).unwrap();
		assert!(
			transcript.contains(block),
			"demos/README.md's {name} excerpt is not in its transcript"
		);
	}
}

/// Runs a demo on another data file of the same shape, through the real executable.
fn run_on(name: &str, json: &str) -> String {
	let dir = std::env::temp_dir().join(format!("rnx-0155-{}-{name}", std::process::id()));
	std::fs::create_dir_all(&dir).unwrap();
	let path = dir.join(format!("{}.json", json.len()));
	std::fs::write(&path, json).unwrap();
	let out = Command::new(env!("CARGO_BIN_EXE_rnx"))
		.args(["run", &format!("demos/{name}.rn"), path.to_str().unwrap()])
		.current_dir(root())
		.output()
		.unwrap();
	assert!(
		out.status.success(),
		"{name} on {json}: exit {:?}",
		out.status
	);
	assert!(
		out.stderr.is_empty(),
		"{name} on {json}: stderr {}",
		String::from_utf8_lossy(&out.stderr)
	);
	String::from_utf8(out.stdout).unwrap()
}

/// The optional input file (review round 1): no orders, one customer, and no
/// shipped revenue are reported plainly, never indexed past or divided by zero.
#[test]
fn edge_inputs_are_handled() {
	let empty = "[]";
	let open = r#"[{"id": 1, "customer": "Elm", "region": "west", "qty": 2, "unit_cents": 100, "status": "open"}]"#;
	let single = r#"[{"id": 1, "customer": "Solo", "region": "west", "qty": 1, "unit_cents": 100, "status": "shipped"}]"#;
	let cancelled = r#"[{"id": 1, "customer": "Ash", "region": "east", "qty": 3, "unit_cents": 0, "status": "cancelled"}]"#;

	let o = run_on("02_orders", empty);
	assert!(
		o.starts_with("0 orders read from ") && o.lines().count() == 1,
		"{o}"
	);
	let o = run_on("02_orders", open);
	assert!(
		o.contains("Largest customer: none (nothing shipped yet)")
			&& o.contains("Still open: 1 of 1 orders (100.0%)"),
		"{o}"
	);
	let o = run_on("02_orders", single);
	assert!(
		o.contains("Largest customer: Solo (1.00), the only customer"),
		"{o}"
	);

	for (json, rows) in [(empty, 0), (open, 1), (cancelled, 1)] {
		let r = run_on("03_report", json);
		let total = r.lines().last().unwrap();
		assert!(
			total.starts_with("total") && total.trim_end().ends_with('-'),
			"{r}"
		);
		assert!(!r.contains('#'), "bars with no shipped revenue: {r}");
		assert_eq!(r.lines().count(), 4 + rows, "{r}");
	}
	let r = run_on("03_report", single);
	assert!(
		r.contains("100.0%  ####################") && r.lines().last().unwrap().ends_with("100.0%"),
		"{r}"
	);
}
