//! Record 0136's chunking measurements, on 0131's pinned model.
//!
//! - `probe0136 chunks MODEL D1 D2 OVERLAP`: every passage's byte range, D1
//!   bodies (as U1 splits a document) then D2 tickets (title, newline,
//!   body), in `twin0133 chunks`'s format, for an exact comparison. Each
//!   plan is checked (substrings on character boundaries, strict progress,
//!   no gaps, every atom covered, each passage within the model's limit once
//!   re-tokenized) and summarized on stderr; exits 1 on any failure.
//! - `probe0136 generated MODEL OVERLAP`: an adversarial corpus. A plan
//!   either covers its text completely, checked as above, or is a named
//!   refusal; the two are reported separately.
//! - `probe0136 truncation MODEL D1 D2`: tokens lost to the model's limit,
//!   by `count_tokens`, for U1's frozen-rule passages and U2's ticket inputs
//!   (unchanged by this record), and for the whole texts.
use rnx_candle::text::{self, chunk};

fn read_d1(dir: &str) -> Vec<(String, String)> {
	let mut names: Vec<String> = std::fs::read_dir(dir)
		.unwrap()
		.map(|e| e.unwrap().file_name().into_string().unwrap())
		.filter(|n| n.ends_with(".md"))
		.collect();
	names.sort();
	names
		.into_iter()
		.map(|n| {
			let text = std::fs::read_to_string(format!("{dir}/{n}")).unwrap();
			let first = text.split('\n').next().unwrap_or("").len();
			(format!("d1:{n}"), text[first..].to_owned())
		})
		.collect()
}
fn read_d2(path: &str) -> Vec<(String, String)> {
	let v: serde_json::Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
	v.as_array()
		.unwrap()
		.iter()
		.map(|i| {
			(
				format!("d2:#{}", i["number"]),
				format!(
					"{}\n{}",
					i["title"].as_str().unwrap(),
					i["body"].as_str().unwrap()
				),
			)
		})
		.collect()
}

#[derive(Default)]
struct Totals {
	texts: usize,
	passages: usize,
	tokens: usize,
	covered: usize,
	repairs: usize,
	fallback_cuts: usize,
	overlap_pairs: usize,
	overlap_actual: usize,
	overlap_short: usize,
	max_tokens: i64,
	over_limit: usize,
	failures: usize,
}

/// The plan's invariants; returns the failures it found.
fn check(
	enc: &text::TextEncoder,
	source: &str,
	body: &str,
	p: &chunk::Plan,
	t: &mut Totals,
) -> usize {
	let max_seq = text::max_seq(enc) as i64;
	let mut bad = 0;
	let mut prev: Option<(usize, usize)> = None;
	let subs: Vec<&str> = p.ranges.iter().map(|&(s, e)| &body[s..e]).collect();
	let counts = if subs.is_empty() {
		vec![]
	} else {
		chunk::count_strs(enc, &subs).unwrap()
	};
	for (k, (&(s, e), &(a, b))) in p.ranges.iter().zip(&p.atoms).enumerate() {
		let ok_range = s < e && body.is_char_boundary(s) && body.is_char_boundary(e) && a < b;
		let ok_order = match prev {
			None => a == 0,
			Some((pa, pb)) => a > pa && a <= pb,
		};
		t.max_tokens = t.max_tokens.max(counts[k]);
		let ok_len = counts[k] <= max_seq;
		t.over_limit += usize::from(!ok_len);
		if !(ok_range && ok_order && ok_len) {
			bad += 1;
			eprintln!(
				"FAIL {source} passage {k}: range {ok_range}, order {ok_order}, {} tokens",
				counts[k]
			);
		}
		prev = Some((a, b));
	}
	let covered = prev.map_or(0, |(_, b)| b);
	if covered != p.atom_count {
		bad += 1;
		eprintln!("FAIL {source}: {covered} of {} atoms covered", p.atom_count);
	}
	t.texts += 1;
	t.passages += p.ranges.len();
	t.tokens += p.tokens;
	t.covered += usize::from(covered == p.atom_count);
	t.repairs += p.repairs;
	t.fallback_cuts += p.fallback_cuts;
	t.overlap_pairs += p.overlaps.len();
	t.overlap_actual += p.overlaps.iter().sum::<usize>();
	t.overlap_short += p.overlaps.iter().filter(|&&o| o < p.overlap).count();
	t.failures += bad;
	bad
}

fn summary(label: &str, overlap: usize, t: &Totals) {
	eprintln!(
		"{label}: {} texts, {} passages, {} content tokens; complete coverage {} of {}; longest passage {} tokens with specials, {} over the limit; repairs {}, atom-fallback cuts {}; overlap requested {overlap}, actual mean {:.1} over {} pairs ({} below the request); failures {}",
		t.texts,
		t.passages,
		t.tokens,
		t.covered,
		t.texts,
		t.max_tokens,
		t.over_limit,
		t.repairs,
		t.fallback_cuts,
		if t.overlap_pairs == 0 {
			0.0
		} else {
			t.overlap_actual as f64 / t.overlap_pairs as f64
		},
		t.overlap_pairs,
		t.overlap_short,
		t.failures
	);
}

fn chunks(model: &str, d1: &str, d2: &str, overlap: i64) -> bool {
	let enc = text::load_dir(model).unwrap();
	println!("source\tindex\tstart\tend");
	let mut ok = true;
	for (label, set) in [("D1", read_d1(d1)), ("D2", read_d2(d2))] {
		let mut t = Totals::default();
		for (source, body) in &set {
			let p = chunk::plan(&enc, body, overlap).unwrap_or_else(|e| panic!("{source}: {e}"));
			for (k, (s, e)) in p.ranges.iter().enumerate() {
				println!("{source}\t{k}\t{s}\t{e}");
			}
			check(&enc, source, body, &p, &mut t);
		}
		summary(label, overlap as usize, &t);
		ok &= t.failures == 0 && t.over_limit == 0 && t.covered == t.texts;
	}
	ok
}

fn generated(model: &str, overlap: i64) -> bool {
	let enc = text::load_dir(model).unwrap();
	let cases: Vec<(&str, String)> = vec![
		("one long word", "supercalifragilistic".repeat(60)),
		(
			"long words among short",
			format!("a {} b {} c", "x".repeat(400), "y".repeat(90)),
		),
		("digits", "1234567890".repeat(300)),
		("digits spaced", "12 345 6789 ".repeat(300)),
		("punctuation run", "!?.,;:-()[]".repeat(300)),
		(
			"punctuation in words",
			"state-of-the-art, e.g. (sic) foo.bar/baz ".repeat(120),
		),
		("CJK", "漢字仮名交じり文".repeat(300)),
		("emoji", "😀🎉👍🏽".repeat(300)),
		("combining accents", "e\u{301}a\u{308}o\u{302} ".repeat(400)),
		(
			"mixed scripts",
			"naïve café Ελληνικά русский العربية हिन्दी 😀 ".repeat(150),
		),
		("whitespace runs", "word \n\n\t  word\r\n".repeat(400)),
		(
			"special-like literals",
			"[CLS] text [SEP] more [PAD] [UNK] [MASK] ".repeat(150),
		),
		("URLs", "https://example.com/a/b?c=d&e=f#g ".repeat(200)),
		(
			"code",
			"fn main() { let x = vec![1, 2, 3]; println!(\"{x:?}\"); } ".repeat(150),
		),
		("only spaces", " ".repeat(5000)),
		("empty", String::new()),
		("one character", "é".to_owned()),
	];
	let mut t = Totals::default();
	let mut refused = Vec::new();
	for (name, body) in &cases {
		match chunk::plan(&enc, body, overlap) {
			Ok(p) => {
				check(&enc, name, body, &p, &mut t);
			}
			Err(e) => refused.push(format!("{name}: {e}")),
		}
	}
	summary("generated, complete plans", overlap as usize, &t);
	eprintln!("generated, named refusals: {}", refused.len());
	for r in &refused {
		eprintln!("  {r}");
	}
	t.failures == 0 && t.over_limit == 0 && t.covered == t.texts
}

fn truncation(model: &str, d1: &str, d2: &str) {
	let enc = text::load_dir(model).unwrap();
	let max_seq = text::max_seq(&enc) as i64;
	// whole documents can exceed count_tokens' per-text limit (embed's), so
	// a text above it is counted by its chunk plan: content tokens plus the
	// model's special tokens
	let report = |label: &str, texts: Vec<String>| {
		let mut counts = Vec::new();
		for t in &texts {
			counts.push(if t.len() <= 65536 {
				chunk::count_strs(&enc, &[t.as_str()]).unwrap()[0]
			} else {
				chunk::plan(&enc, t, 0).unwrap().tokens as i64 + 2
			});
		}
		let over = counts.iter().filter(|&&c| c > max_seq).count();
		let lost: i64 = counts.iter().map(|&c| (c - max_seq).max(0)).sum();
		let total: i64 = counts.iter().sum();
		println!(
			"{label}: {} inputs; over {max_seq} tokens: {over} ({:.1}%); tokens lost to truncation: {lost} of {total} ({:.1}%)",
			counts.len(),
			100.0 * over as f64 / counts.len() as f64,
			100.0 * lost as f64 / total as f64
		);
	};
	report(
		"D1 whole document bodies",
		read_d1(d1).into_iter().map(|x| x.1).collect(),
	);
	report(
		"D2 whole tickets (title, newline, body)",
		read_d2(d2).into_iter().map(|x| x.1).collect(),
	);
	// U2's ticket inputs, unchanged by this record: title, newline, the first
	// 180 words of the body
	let v: serde_json::Value = serde_json::from_slice(&std::fs::read(d2).unwrap()).unwrap();
	let u2: Vec<String> = v
		.as_array()
		.unwrap()
		.iter()
		.map(|i| {
			let ws: Vec<&str> = i["body"]
				.as_str()
				.unwrap()
				.split([' ', '\n', '\t', '\r'])
				.filter(|w| !w.is_empty())
				.collect();
			format!(
				"{}\n{}",
				i["title"].as_str().unwrap(),
				ws[..ws.len().min(180)].join(" ")
			)
		})
		.collect();
	report("D2 ticket inputs as U2 builds them (unchanged)", u2);
}

fn main() {
	let a: Vec<String> = std::env::args().collect();
	let ok = match a[1].as_str() {
		"chunks" => chunks(&a[2], &a[3], &a[4], a[5].parse().unwrap()),
		"generated" => generated(&a[2], a[3].parse().unwrap()),
		"truncation" => {
			truncation(&a[2], &a[3], &a[4]);
			true
		}
		other => panic!("unknown command {other}"),
	};
	if !ok {
		std::process::exit(1);
	}
}
