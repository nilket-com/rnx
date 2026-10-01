//! Record 0136: token-aware chunking on a loaded encoder.
//!
//! - `enc.count_tokens(texts)`: each text's token count with the model's
//!   special tokens, untruncated.
//! - `enc.chunk(text, overlap)`: the text split into passages that `embed`
//!   takes without truncation, on word boundaries where a word fits, each
//!   the original text's substring.
//!
//! Both tokenize with a per-call copy of the encoder's tokenizer (no
//! truncation, no padding), so `embed`'s configuration never changes, and
//! both run inside the joined worker's panic boundary. Offsets are UTF-8
//! byte offsets, validated; tokens are grouped into atoms (runs whose byte
//! ranges overlap, repeat or are empty), and cuts fall only between atoms,
//! so every cut is on a character boundary.
use super::{Inner, MAX_TEXT, MAX_TEXTS, MAX_TOTAL, TextEncoder};
use crate::worker;
use rnx::rune::{self, runtime::Vec as RuneVec};
use tokenizers::{PostProcessor, Tokenizer};

/// One chunked text is a whole document, so it has its own bound, above
/// `embed`'s per-text `MAX_TEXT`: real records exceed 64 KiB (D1's largest
/// is 102,776 bytes). Each passage is still at most `MAX_TEXT`.
pub const MAX_DOCUMENT: usize = 1 << 20;
/// The token table of one chunked text, checked: a tokenizer's output count
/// is not bounded by the input's bytes alone.
pub const MAX_TOKENS: usize = 1 << 18;
/// Word (or atom) removals while a passage's re-tokenized count is above the
/// model's limit, per passage.
pub const REPAIRS: usize = 16;

/// The limits one call plans under. Production always uses `LIMITS`; the
/// test-support entry lowers them, or inflates the re-measured count, so the
/// controls reach each refusal (0132's per-call knobs pattern).
#[derive(Clone, Copy, Debug)]
pub struct Limits {
	pub document: usize,
	pub tokens: usize,
	/// One passage's bytes: `embed`'s per-text limit.
	pub text: usize,
	pub texts: usize,
	pub total: usize,
	pub repairs: usize,
	/// Added to every re-measured passage count: forces repair.
	pub inflate: usize,
}
pub const LIMITS: Limits = Limits {
	document: MAX_DOCUMENT,
	tokens: MAX_TOKENS,
	text: MAX_TEXT,
	texts: MAX_TEXTS,
	total: MAX_TOTAL,
	repairs: REPAIRS,
	inflate: 0,
};

/// A per-call tokenizer: the encoder's, without truncation or padding.
fn per_call(inner: &Inner, op: &str) -> Result<Tokenizer, String> {
	let mut t = inner.tokenizer.clone();
	t.with_padding(None);
	t.with_truncation(None)
		.map_err(|e| format!("{op}: truncation: {e}"))?;
	Ok(t)
}

/// The content tokens the model takes per passage: its sequence length less
/// the special tokens its post-processor adds to one sequence.
fn window(inner: &Inner) -> usize {
	let added = inner
		.tokenizer
		.get_post_processor()
		.map_or(0, |p| p.added_tokens(false));
	inner.max_seq - added
}

/// `enc.count_tokens(texts)`.
pub(super) fn count_tokens(this: &TextEncoder, texts: rune::Value) -> Result<Vec<i64>, String> {
	let op = "TextEncoder::count_tokens";
	let not_texts = || format!("{op}: texts must be a vector of strings");
	let values = texts.borrow_ref::<RuneVec>().map_err(|_| not_texts())?;
	// the count first, before a table per text exists (embed's order)
	let n = values.len();
	if n == 0 || n > MAX_TEXTS {
		return Err(format!("{op}: {n} texts, want 1 to {MAX_TEXTS}"));
	}
	let mut guards = Vec::with_capacity(n);
	for v in values.iter() {
		guards.push(v.borrow_string_ref().map_err(|_| not_texts())?);
	}
	let strs: Vec<&str> = guards.iter().map(|s| &**s).collect();
	count_strs(this, &strs)
}

pub fn count_strs(this: &TextEncoder, strs: &[&str]) -> Result<Vec<i64>, String> {
	let op = "TextEncoder::count_tokens";
	super::preflight(strs, 1, op)?;
	let inner = &*this.0;
	worker::run(op, || -> Result<Vec<i64>, String> {
		let t = per_call(inner, op)?;
		strs.iter()
			.map(|s| {
				t.encode(*s, true)
					.map(|e| e.len() as i64)
					.map_err(|e| format!("{op}: {e}"))
			})
			.collect()
	})?
}

/// One atom: a run of tokens whose byte ranges overlap, repeat or are
/// empty, as one unbreakable unit.
#[derive(Clone, Copy, Debug)]
struct Atom {
	start: usize,
	end: usize,
	tokens: usize,
	/// The index of the word this atom belongs to.
	word: usize,
}

/// A chunking plan: every passage's byte range, decided and checked before
/// any string is built.
#[derive(Clone, Debug, Default)]
pub struct Plan {
	/// Each passage's byte range in the input.
	pub ranges: Vec<(usize, usize)>,
	/// Each passage's atom range, `[first, end)`.
	pub atoms: Vec<(usize, usize)>,
	/// The text's atom count, and its content-token count.
	pub atom_count: usize,
	pub tokens: usize,
	/// The content tokens per passage (by the original encoding).
	pub passage_tokens: Vec<usize>,
	/// For each consecutive pair, the overlap actually taken, in tokens.
	pub overlaps: Vec<usize>,
	/// Passages cut inside a word (the atom fallback).
	pub fallback_cuts: usize,
	/// Word or atom removals after re-tokenizing, in total.
	pub repairs: usize,
	/// The content window W and the requested overlap.
	pub window: usize,
	pub overlap: usize,
}

/// The atoms and their words, from one encoding's offsets and word ids.
fn atoms(
	text: &str,
	offsets: &[(usize, usize)],
	words: &[Option<u32>],
	ids: &[u32],
	op: &str,
) -> Result<Vec<Atom>, String> {
	let mut out: Vec<Atom> = Vec::new();
	// empty tokens before any atom attach to the first one
	let mut pending = 0usize;
	let mut last_word: Option<Option<u32>> = None;
	let mut word = 0usize;
	for (i, &(s, e)) in offsets.iter().enumerate() {
		if s > e || e > text.len() || !text.is_char_boundary(s) || !text.is_char_boundary(e) {
			return Err(format!(
				"{op}: the tokenizer reported an offset outside the text or inside a character ({s}..{e} of {} bytes, token {})",
				text.len(),
				ids[i]
			));
		}
		let w = words.get(i).copied().flatten();
		// an empty-span token (a trimmed space) is placed in the gap it was
		// trimmed from: after an atom, the atom extends to its offset; before
		// the first atom, the first atom starts at the text's start
		if s == e {
			match out.last_mut() {
				Some(a) => {
					a.tokens += 1;
					a.end = a.end.max(s);
				}
				None => pending += 1,
			}
			continue;
		}
		if let Some(a) = out.last_mut()
			&& s < a.end
		{
			a.start = a.start.min(s);
			a.end = a.end.max(e);
			a.tokens += 1;
			continue;
		}
		// a new atom; a new word unless it shares the previous word id
		let same = matches!((last_word, w), (Some(Some(p)), Some(q)) if p == q);
		if !out.is_empty() && !same {
			word += 1;
		}
		last_word = Some(w);
		// tokens pending before the first atom are its leading trimmed
		// spaces: it starts at the text's start, covering that gap
		let lead = std::mem::take(&mut pending);
		out.push(Atom {
			start: if lead > 0 { 0 } else { s },
			end: e,
			tokens: 1 + lead,
			word,
		});
	}
	if pending > 0 {
		// only possible with no atom at all: tokens with no span in the text
		// cannot be placed in a passage, which is a substring; refused, never
		// silently dropped
		return Err(format!(
			"{op}: the tokenizer gives {pending} tokens with no span in the text (for example a trimmed space); they cannot be placed in a passage"
		));
	}
	// every token of the encoding is in exactly one atom
	let held: usize = out.iter().map(|a| a.tokens).sum();
	if held != offsets.len() {
		return Err(format!(
			"{op}: the atoms hold {held} of the encoding's {} tokens",
			offsets.len()
		));
	}
	Ok(out)
}

/// The plan for one text: atoms, then passages of whole words, an atom
/// fallback for a word that cannot fit, re-tokenization repair, and the
/// next start from the repaired end.
pub fn plan(this: &TextEncoder, text: &str, overlap: i64) -> Result<Plan, String> {
	plan_limited(this, text, overlap, LIMITS)
}

/// Test support: `plan` under lowered limits, or with repair forced.
#[cfg(any(test, feature = "test-support"))]
pub fn plan_with(
	this: &TextEncoder,
	text: &str,
	overlap: i64,
	limits: Limits,
) -> Result<Plan, String> {
	plan_limited(this, text, overlap, limits)
}

fn plan_limited(
	this: &TextEncoder,
	text: &str,
	overlap: i64,
	limits: Limits,
) -> Result<Plan, String> {
	let op = "TextEncoder::chunk";
	let inner = &*this.0;
	if text.len() > limits.document {
		return Err(format!(
			"{op}: the text is {} bytes, at most {}",
			text.len(),
			limits.document
		));
	}
	let w = window(inner);
	let overlap = usize::try_from(overlap)
		.ok()
		.filter(|o| *o <= w / 2)
		.ok_or_else(|| format!("{op}: overlap {overlap}, want 0 to {} tokens", w / 2))?;
	worker::run(op, || plan_inner(inner, text, w, overlap, limits, op))?
}

fn plan_inner(
	inner: &Inner,
	text: &str,
	w: usize,
	overlap: usize,
	lim: Limits,
	op: &str,
) -> Result<Plan, String> {
	let t = per_call(inner, op)?;
	let enc = t.encode(text, false).map_err(|e| format!("{op}: {e}"))?;
	if enc.len() > lim.tokens {
		return Err(format!(
			"{op}: the text is {} tokens, at most {}",
			enc.len(),
			lim.tokens
		));
	}
	let atoms = atoms(
		text,
		enc.get_offsets(),
		enc.get_word_ids(),
		enc.get_ids(),
		op,
	)?;
	drop(enc);
	let mut p = Plan {
		atom_count: atoms.len(),
		tokens: atoms.iter().map(|a| a.tokens).sum(),
		window: w,
		overlap,
		..Plan::default()
	};
	let n = atoms.len();
	// the atom index where each atom's word ends
	let mut word_end = vec![n; n];
	for i in (0..n).rev() {
		if i + 1 < n && atoms[i + 1].word == atoms[i].word {
			word_end[i] = word_end[i + 1];
		} else {
			word_end[i] = i + 1;
		}
	}
	let word_start = |i: usize| i == 0 || atoms[i - 1].word != atoms[i].word;
	let tokens = |a: usize, b: usize| atoms[a..b].iter().map(|x| x.tokens).sum::<usize>();
	let mut total = 0usize;
	let mut c = 0usize;
	while c < n {
		// whole words (the first may be the rest of a word cut before)
		let mut e = c;
		let mut used = 0usize;
		while e < n {
			let end = word_end[e];
			let more = tokens(e, end);
			if used + more > w {
				break;
			}
			used += more;
			e = end;
		}
		let mut fallback = false;
		if e == c {
			// a word that cannot fit: whole atoms, each a character run
			fallback = true;
			while e < n && used + atoms[e].tokens <= w {
				used += atoms[e].tokens;
				e += 1;
			}
			if e == c {
				return Err(format!(
					"{op}: text has a single unbreakable run of {} tokens at bytes {}..{}, above the model's {w}",
					atoms[c].tokens, atoms[c].start, atoms[c].end
				));
			}
		}
		// repair: the passage's own tokenization, with specials, must fit
		let mut removed = 0usize;
		loop {
			let sub = &text[atoms[c].start..atoms[e - 1].end];
			let count = t.encode(sub, true).map_err(|x| format!("{op}: {x}"))?.len() + lim.inflate;
			if count <= inner.max_seq {
				break;
			}
			removed += 1;
			if removed > lim.repairs {
				return Err(format!(
					"{op}: the passage at bytes {}..{} still re-tokenizes to {count} tokens after {} removals, above {}",
					atoms[c].start,
					atoms[e - 1].end,
					lim.repairs,
					inner.max_seq
				));
			}
			// the last word, or the last atom in the fallback
			let cut = if fallback {
				e - 1
			} else {
				(c..e).rev().find(|&i| word_start(i) && i > c).unwrap_or(c)
			};
			if cut <= c {
				return Err(format!(
					"{op}: nothing of the passage at bytes {}..{} fits the model's {} tokens once re-tokenized",
					atoms[c].start,
					atoms[e - 1].end,
					inner.max_seq
				));
			}
			e = cut;
		}
		p.repairs += removed;
		p.fallback_cuts += usize::from(fallback && e < n && !word_start(e));
		let (start, end) = (atoms[c].start, atoms[e - 1].end);
		if end - start > lim.text {
			return Err(format!(
				"{op}: the passage at bytes {start}..{end} is {} bytes, above {} (embed's per-text limit)",
				end - start,
				lim.text
			));
		}
		if p.ranges.len() + 1 > lim.texts {
			return Err(format!("{op}: more than {} passages", lim.texts));
		}
		total = total
			.checked_add(end - start)
			.filter(|x| *x <= lim.total)
			.ok_or_else(|| format!("{op}: the passages exceed {} bytes", lim.total))?;
		p.ranges.push((start, end));
		p.atoms.push((c, e));
		p.passage_tokens.push(tokens(c, e));
		if e == n {
			break;
		}
		// the next start, from the repaired end: back by at most `overlap`
		// tokens, but strictly after this start; then forward to a word
		// start, but no further than the end
		let mut back = e;
		let mut taken = 0usize;
		while back > c + 1 && taken + atoms[back - 1].tokens <= overlap {
			back -= 1;
			taken += atoms[back].tokens;
		}
		let mut next = back;
		while next < e && !word_start(next) {
			next += 1;
		}
		debug_assert!(next > c && next <= e);
		p.overlaps.push(tokens(next, e));
		c = next;
	}
	Ok(p)
}

/// `enc.chunk(text, overlap)`: the plan, checked, then its substrings.
pub(super) fn chunk(this: &TextEncoder, text: &str, overlap: i64) -> Result<Vec<String>, String> {
	let p = plan(this, text, overlap)?;
	Ok(p.ranges
		.iter()
		.map(|&(s, e)| text[s..e].to_owned())
		.collect())
}
