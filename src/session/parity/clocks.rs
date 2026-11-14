//! Clock readings are compared within each session, never across origins.
use crate::clock_properties::Property;
use serde_json::{Value, json};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

fn integer(observation: &Value) -> Option<i64> {
	if observation["outcome"] != "ok" {
		return None;
	}
	let text = observation["value"].as_str()?;
	let value = text.parse::<i64>().ok()?;
	(text == value.to_string()).then_some(value)
}
fn wall_ms() -> i64 {
	SystemTime::now()
		.duration_since(UNIX_EPOCH)
		.unwrap()
		.as_millis()
		.try_into()
		.unwrap()
}
struct Observation {
	normalized: Value,
	raw: Value,
	valid: bool,
}
fn capture(property: Property, function: &str, mut eval: impl FnMut(&str) -> Value) -> Observation {
	let names = match property {
		Property::WallEnvelope { .. } => vec!["clock_a"],
		_ => vec!["clock_a", "clock_b"],
	};
	let mut readings = Vec::new();
	let mut windows = Vec::new();
	let origin = Instant::now();
	let wall_before = wall_ms();
	for (index, name) in names.iter().enumerate() {
		if index != 0
			&& let Property::MonotonicPairEnvelope { pause_ms, .. } = property
		{
			std::thread::sleep(Duration::from_millis(pause_ms));
		}
		let start = origin.elapsed().as_nanos();
		let result = eval(&format!("let {name}={function}();{name}"));
		let end = origin.elapsed().as_nanos();
		readings.push(result);
		windows.push([start, end]);
	}
	let wall_after = wall_ms();
	let mut types = Vec::new();
	let mut bindings = Vec::new();
	for name in &names {
		types.push(eval(&format!("{name} is i64")));
		bindings.push(eval(name));
	}
	let values: Vec<_> = readings.iter().map(integer).collect();
	let (gap, span) = if windows.len() == 2 {
		(windows[1][0] - windows[0][1], windows[1][1] - windows[0][0])
	} else {
		(0, 0)
	};
	let valid = property.check(&values, [wall_before, wall_after], gap, span)
		&& types
			.iter()
			.all(|v| *v == json!({"outcome":"ok","value":"true"}))
		&& bindings == readings;
	let raw = json!({"readings":readings,"types":types,"bindings":bindings,"wall":[wall_before,wall_after],"windows_ns":windows,"valid":valid});
	// Only replace verified numeric slots; all other fields remain observed.
	if valid {
		for v in readings.iter_mut().chain(bindings.iter_mut()) {
			v["value"] = "<verified clock i64>".into();
		}
	}
	Observation {
		normalized: json!({"readings":readings,"types":types,"bindings":bindings,"binding_names":names}),
		raw,
		valid,
	}
}
fn property_class(a: &Observation, b: &Observation) -> &'static str {
	if !a.valid || !b.valid {
		"Property failure"
	} else if a.normalized != b.normalized {
		"Different result"
	} else {
		"Equal by property"
	}
}
pub(crate) fn observe(
	function: &str,
	context: rune::Context,
) -> crate::routing_inventory::Result<Value> {
	use crate::session::{
		DEFAULT_CEILING, Session,
		domain_session::{
			DomainSession,
			transcripts::{parity_new, parity_old},
		},
	};
	let property = Property::for_function(function).ok_or("not a clock property")?;
	let mut old = Session::new(context)?;
	let mut new = DomainSession::for_frontend(
		crate::routing_inventory::session_parity_context()?,
		DEFAULT_CEILING,
		crate::session::remedy::FrontEnd::Terminal,
	)?;
	let a = capture(property, function, |input| parity_old(&mut old, input));
	let b = capture(property, function, |input| parity_new(&mut new, input));
	old.close()?;
	new.close()?;
	let class = property_class(&a, &b);
	let raw = json!({"function":function,"property":property,"class":class,"legacy":a.raw,"domain":b.raw});
	// Opt-in evidence goes into the campaign's fresh per-run directory.
	if let Some(out) = std::env::var_os("RNX_CLOCK_OBSERVATIONS") {
		static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
		let out = std::path::PathBuf::from(out);
		std::fs::create_dir_all(&out)?;
		let path = out.join(format!(
			"{}-{}-{}.json",
			std::process::id(),
			NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
			function.replace("::", "-")
		));
		let mut file = std::fs::OpenOptions::new()
			.write(true)
			.create_new(true)
			.open(path)?;
		serde_json::to_writer_pretty(&mut file, &raw)?;
	}
	let mut row = json!({"function":function,"input":format!("{function}()"),"property":property,"legacy":a.normalized,"domain":b.normalized,"class":class,"wording_rule":null,"domain_worse":false});
	if class != "Equal by property" {
		row["raw_observation"] = raw;
	}
	Ok(row)
}
#[test]
fn classification_and_presentation_teeth() {
	for bad in [
		json!({"outcome":"ok","value":"\"20\""}),
		json!({"outcome":"ok","value":"20.0"}),
		json!({"outcome":"ok","value":"020"}),
		json!({"outcome":"error","value":"20"}),
	] {
		assert_eq!(integer(&bad), None);
	}
	let good = || Observation {
		normalized: json!({"readings":["same"]}),
		raw: Value::Null,
		valid: true,
	};
	let a = good();
	let mut b = good();
	assert_eq!(property_class(&a, &b), "Equal by property");
	b.valid = false;
	assert_eq!(property_class(&a, &b), "Property failure");
	b.valid = true;
	b.normalized["extra"] = "changed".into();
	assert_eq!(property_class(&a, &b), "Different result");
	// A changed binding or type must fail even with otherwise valid wall readings.
	let p = Property::for_function("time::now_ms").unwrap();
	for mode in [0, 1, 2] {
		let mut reading: Option<i64> = None;
		let observed = capture(p, "time::now_ms", |input| {
			if input.ends_with("is i64") {
				json!({"outcome":"ok","value":if mode==1 {"false"}else{"true"}})
			} else if input == "clock_a" {
				json!({"outcome":"ok","value":if mode==2 {"-1".to_owned()}else{reading.unwrap().to_string()}})
			} else {
				let fixed = wall_ms();
				reading = Some(fixed);
				json!({"outcome":"ok","value":fixed.to_string()})
			}
		});
		assert_eq!(observed.valid, mode == 0, "mode {mode}: {}", observed.raw);
	}
}
#[test]
fn clock_rows_execute_by_property() {
	for name in ["time::now_ms", "time::monotonic_ms", "time::monotonic_ns"] {
		let row = observe(
			name,
			crate::routing_inventory::session_parity_context().unwrap(),
		)
		.unwrap();
		assert_eq!(row["class"], "Equal by property", "{row}");
	}
}
