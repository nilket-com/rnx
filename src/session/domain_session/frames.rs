//! Map only user-source segments. Generated wrapper locations are omitted.
use super::*;
use rune::experimental_core::domain::CrossFailure;
use std::fmt::{self, Write};

const FRAME_LIMIT: usize = 32;
const BYTE_LIMIT: usize = 16 * 1024;
const CUT: &str = "\n  [diagnostic truncated]";
struct Text(String);
impl Write for Text {
	fn write_str(&mut self, s: &str) -> fmt::Result {
		let room = (BYTE_LIMIT - CUT.len()).saturating_sub(self.0.len());
		if s.len() <= room {
			self.0.push_str(s);
			return Ok(());
		}
		let mut end = room.min(s.len());
		while !s.is_char_boundary(end) {
			end -= 1;
		}
		self.0.push_str(&s[..end]);
		Err(fmt::Error)
	}
}

pub(super) fn render(
	failure: &CrossFailure<'_>,
	maps: &BTreeMap<u64, SourceMap>,
	inputs: &[String],
	numberings: &[usize],
) -> String {
	let mut text = Text(String::new());
	let mut cut = write!(text, "{}", failure.primary_message()).is_err();
	let mut shown = 0;
	for frame in failure.frames() {
		if cut {
			break;
		}
		let origin = frame.location.and_then(|loc| {
			let known_source = frame
				.program
				.sources()
				.iter()
				.any(|s| s.id == loc.source && s.revision == loc.revision);
			if !known_source {
				return Some(None);
			}
			map_location(
				maps.get(&loc.revision),
				loc.start as usize,
				inputs,
				numberings,
			)
		});
		// A missing location is unavailable, not proof of a wrapper frame.
		let origin = if frame.location.is_none() {
			Some(None)
		} else {
			origin
		};
		let Some(origin) = origin else {
			continue;
		};
		if shown == FRAME_LIMIT {
			cut = true;
			break;
		}
		shown += 1;
		cut = match origin {
			Some(o) => write!(
				text,
				"\n  at input {}, line {}, column {}",
				o.number, o.line, o.column
			),
			None => text.write_str("\n  at unavailable source"),
		}
		.is_err();
	}
	if cut || failure.truncated {
		text.0.push_str(CUT);
	}
	text.0
}

// None omits a known generated wrapper; Some(None) reports unavailable source.
fn map_location(
	map: Option<&SourceMap>,
	start: usize,
	inputs: &[String],
	numberings: &[usize],
) -> Option<Option<Origin>> {
	let Some(map) = map else {
		return Some(None);
	};
	map.locate(start)
		.map(|(i, o)| origin_in(inputs, numberings, i, o))
}

#[cfg(test)]
mod tests {
	use super::*;
	#[test]
	fn wrapper_only_and_missing_sources_are_distinct() {
		let mut generated = Generated::new();
		generated.raw("wrapper ");
		generated.user("1/0", 1, 0);
		let inputs = vec!["1/0".into()];
		assert_eq!(map_location(Some(&generated.map), 0, &inputs, &[0]), None);
		assert!(
			map_location(Some(&generated.map), 8, &inputs, &[0])
				.unwrap()
				.is_some()
		);
		assert_eq!(map_location(None, 0, &inputs, &[0]), Some(None));
		assert_eq!(map_location(Some(&generated.map), 8, &[], &[0]), Some(None));
	}
	#[test]
	fn byte_bound_preserves_utf8_and_marker_space() {
		let mut text = Text(String::new());
		assert!(write!(text, "{}", "é".repeat(BYTE_LIMIT)).is_err());
		text.0.push_str(CUT);
		assert!(text.0.len() <= BYTE_LIMIT);
		assert!(text.0.ends_with(CUT));
	}
}
