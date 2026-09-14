//! Two clocks and a calendar over integer milliseconds: record 0038.
use crate::host::HostFunction;
use jiff::{
	Timestamp, Zoned,
	civil::DateTime,
	tz::{AmbiguousOffset, Offset, OffsetConflict, TimeZone},
};
use rune::{
	Context, Module,
	runtime::{Object, Value},
};
use std::{
	sync::OnceLock,
	time::{Duration, Instant},
};
const MIN_MS: i64 = -377_705_023_201_000;
const MAX_MS: i64 = 253_402_207_200_999;
const MAX_SLEEP_MS: i64 = 68_719_476_735;

fn timestamp(ms: i64) -> Result<Timestamp, String> {
	if !(MIN_MS..=MAX_MS).contains(&ms) {
		return Err(format!(
			"moment {ms} milliseconds is outside {MIN_MS}..={MAX_MS}"
		));
	}
	// Unlike from_millisecond, this preserves the last 999 ms of MAX.
	Timestamp::new(
		ms.div_euclid(1000),
		(ms.rem_euclid(1000) * 1_000_000) as i32,
	)
	.map_err(|e| format!("cannot read moment {ms}: {e}"))
}
fn milliseconds(ts: Timestamp) -> i64 {
	// Jiff's seconds/subseconds are both signed and truncated toward zero.
	ts.as_second() * 1000 + i64::from(ts.subsec_nanosecond()).div_euclid(1_000_000)
}
fn now_ms() -> i64 {
	milliseconds(Timestamp::now())
}
fn monotonic_ms() -> i64 {
	static ORIGIN: OnceLock<Instant> = OnceLock::new();
	// Saturation can only occur after ~292 million years in one process.
	ORIGIN
		.get_or_init(Instant::now)
		.elapsed()
		.as_millis()
		.min(i64::MAX as u128) as i64
}
fn fixed_offset(s: &str) -> Result<Offset, String> {
	let b = s.as_bytes();
	if b.len() != 6
		|| !matches!(b[0], b'+' | b'-')
		|| b[3] != b':'
		|| ![b[1], b[2], b[4], b[5]].iter().all(u8::is_ascii_digit)
	{
		return Err(format!(
			"offset {s:?} must be ±HH:MM with hours 00..23 and minutes 00..59"
		));
	}
	let hours = i32::from(b[1] - b'0') * 10 + i32::from(b[2] - b'0');
	let minutes = i32::from(b[4] - b'0') * 10 + i32::from(b[5] - b'0');
	if hours > 23 || minutes > 59 {
		return Err(format!(
			"offset {s:?} must have hours 00..23 and minutes 00..59"
		));
	}
	Offset::from_seconds((hours * 3600 + minutes * 60) * if b[0] == b'-' { -1 } else { 1 })
		.map_err(|e| e.to_string())
}
fn zone(name: &str) -> Result<TimeZone, String> {
	let result = if name == "UTC" {
		Ok(TimeZone::UTC)
	} else if name == "local" {
		TimeZone::try_system()
	} else if name.starts_with(['+', '-']) {
		return fixed_offset(name).map(TimeZone::fixed);
	} else {
		TimeZone::get(name)
	};
	result.map_err(|e| {
		if name == "local" {
			format!(
				"cannot resolve zone {name:?} from TZ={:?} or the system configuration (TZDIR={:?}): {e}",
				std::env::var_os("TZ"),
				std::env::var_os("TZDIR")
			)
		} else {
			format!("cannot resolve zone {name:?}: {e}")
		}
	})
}
fn zoned(ms: i64, name: &str) -> Result<Zoned, String> {
	Ok(timestamp(ms)?.to_zoned(zone(name)?))
}
fn format(ms: i64, pattern: &str, name: &str) -> Result<String, String> {
	jiff::fmt::strtime::format(pattern, &zoned(ms, name)?).map_err(|e| {
		format!("cannot format moment {ms} in zone {name:?} with pattern {pattern:?}: {e}")
	})
}
fn rfc3339(ms: i64, name: &str) -> Result<String, String> {
	let z = zoned(ms, name)?;
	if !(0..=9999).contains(&z.year()) {
		return Err(format!(
			"cannot write RFC 3339 year {} for moment {ms} in zone {name:?}: requires 0000..9999",
			z.year()
		));
	}
	let offset = z.offset().seconds();
	if offset % 60 != 0 || offset.abs() > 23 * 3600 + 59 * 60 {
		return Err(format!(
			"cannot write RFC 3339 offset {} for moment {ms} in zone {name:?}: requires whole minutes in ±00:00..±23:59",
			z.offset()
		));
	}
	let pattern = if name == "UTC" || z.time_zone().iana_name() == Some("UTC") {
		"%Y-%m-%dT%H:%M:%S%.3fZ"
	} else {
		"%Y-%m-%dT%H:%M:%S%.3f%:z"
	};
	jiff::fmt::strtime::format(pattern, &z)
		.map_err(|e| format!("cannot write RFC 3339 moment {ms}: {e}"))
}

/// Check only the syntax Jiff deliberately erases or broadens. Jiff remains
/// responsible for all date/time validation. Work on bytes, never UTF-8 slices
/// at arbitrary offsets. Accept Jiff's extended/basic time forms, but offsets
/// must use the record's ±HH:MM spelling. All annotations must be one named zone.
fn parse(text: &str) -> Result<i64, String> {
	let refuse = |why: String| format!("cannot parse time {text:?}: {why}");
	let (body, annotation) = if let Some((body, tail)) = text.split_once('[') {
		let name = tail
			.strip_suffix(']')
			.ok_or_else(|| refuse("unterminated zone annotation".into()))?;
		let name = name.strip_prefix('!').unwrap_or(name);
		if name.is_empty() || name.contains(['[', ']', '=']) || name.starts_with(['+', '-']) {
			return Err(refuse(format!(
				"annotation {tail:?} must be one named time zone"
			)));
		}
		TimeZone::get(name)
			.map_err(|e| refuse(format!("unknown zone annotation {name:?}: {e}")))?;
		(body, true)
	} else {
		(text, false)
	};
	let start = body
		.find(['T', 't', ' '])
		.ok_or_else(|| refuse("expected a date-time with an offset or Z".into()))?
		+ 1;
	let time_offset = &body[start..];
	let end = time_offset
		.find(['Z', 'z', '+', '-'])
		.ok_or_else(|| refuse("an offset or Z is required".into()))?;
	let suffix = &time_offset[end..];
	let numeric_offset = if suffix == "Z" || suffix == "z" {
		None
	} else {
		Some(fixed_offset(suffix).map_err(&refuse)?)
	};
	let clock = time_offset[..end].split(['.', ',']).next().unwrap_or("");
	let seconds = if clock.contains(':') {
		clock.split(':').nth(2).map(str::as_bytes)
	} else if clock.len() == 6 {
		Some(&clock.as_bytes()[4..6])
	} else {
		None
	};
	if seconds == Some(b"60".as_slice()) {
		return Err(refuse("leap second 60 is not supported".into()));
	}
	let ts = if annotation {
		let z = text.parse::<Zoned>().map_err(|e| refuse(e.to_string()))?;
		// Jiff permits minute-rounded historical offsets. This contract
		// requires exact agreement for numeric offsets; Z is exempt.
		if let Some(offset) = numeric_offset {
			if offset != z.offset() {
				return Err(refuse(format!(
					"numeric offset {offset} disagrees with zone offset {}",
					z.offset()
				)));
			}
		}
		Ok(z.timestamp())
	} else {
		text.parse::<Timestamp>()
	}
	.map_err(|e| refuse(format!("{e}; supported milliseconds {MIN_MS}..={MAX_MS}")))?;
	Ok(milliseconds(ts))
}
fn parts(ms: i64, name: &str) -> Result<Value, String> {
	let z = zoned(ms, name)?;
	let mut object = Object::new();
	for (key, n) in [
		("year", i64::from(z.year())),
		("month", i64::from(z.month())),
		("day", i64::from(z.day())),
		("hour", i64::from(z.hour())),
		("minute", i64::from(z.minute())),
		("second", i64::from(z.second())),
		("millisecond", i64::from(z.millisecond())),
		("weekday", i64::from(z.weekday().to_monday_one_offset())),
		("offset_seconds", i64::from(z.offset().seconds())),
	] {
		object
			.insert(
				rune::alloc::String::try_from(key)
					.map_err(|e| format!("cannot build time parts: {e}"))?,
				Value::from(n),
			)
			.map_err(|e| format!("cannot build time parts: {e}"))?;
	}
	// Local POSIX rules or unnamed TZif data may have no IANA identifier.
	// Keep "local" in that case, rather than mislabelling a changing zone
	// as the fixed offset it happens to have at this instant.
	let resolved =
		z.time_zone()
			.iana_name()
			.unwrap_or(if name == "local" { "local" } else { name });
	object
		.insert(
			rune::alloc::String::try_from("zone")
				.map_err(|e| format!("cannot build time parts: {e}"))?,
			rune::to_value(resolved.to_owned())
				.map_err(|e| format!("cannot build time parts: {e}"))?,
		)
		.map_err(|e| format!("cannot build time parts: {e}"))?;
	rune::to_value(object).map_err(|e| format!("cannot build time parts: {e}"))
}
fn field(
	object: &Object,
	name: &str,
	default: Option<i64>,
	min: i64,
	max: i64,
) -> Result<i64, String> {
	let n = match object.get(name) {
		Some(value) => value
			.as_integer::<i64>()
			.map_err(|e| format!("time field {name:?} must be an integer: {e}"))?,
		None => default.ok_or_else(|| format!("missing time field {name:?}"))?,
	};
	if !(min..=max).contains(&n) {
		return Err(format!("time field {name:?} is {n}, outside {min}..={max}"));
	}
	Ok(n)
}
fn from_parts(object: &Object, name: &str) -> Result<i64, String> {
	let year = field(object, "year", None, -9999, 9999)? as i16;
	let month = field(object, "month", None, 1, 12)? as i8;
	let day = field(object, "day", None, 1, 31)? as i8;
	let hour = field(object, "hour", Some(0), 0, 23)? as i8;
	let minute = field(object, "minute", Some(0), 0, 59)? as i8;
	let second = field(object, "second", Some(0), 0, 59)? as i8;
	let subsec = field(object, "millisecond", Some(0), 0, 999)? as i32 * 1_000_000;
	let dt = DateTime::new(year, month, day, hour, minute, second, subsec).map_err(|e| {
		format!("cannot build time fields year={year} month={month} day={day}: {e}")
	})?;
	let tz = zone(name)?;
	let amb = tz.to_ambiguous_zoned(dt);
	if let AmbiguousOffset::Gap { .. } = amb.offset() {
		return Err(format!(
			"time reading {dt} in zone {name:?} is in a gap: nonexistent with any offset"
		));
	}
	let result = if object.contains_key("offset_seconds") {
		let off = field(object, "offset_seconds", None, -93599, 93599)? as i32;
		OffsetConflict::Reject
			.resolve(
				dt,
				Offset::from_seconds(off).map_err(|e| e.to_string())?,
				tz,
			)
			.and_then(|a| a.unambiguous())
	} else {
		if let AmbiguousOffset::Fold { .. } = amb.offset() {
			return Err(format!(
				"time reading {dt} in zone {name:?} is in a fold: supply offset_seconds to select an occurrence"
			));
		}
		amb.unambiguous()
	};
	result
		.map(|z| milliseconds(z.timestamp()))
		.map_err(|e| format!("cannot resolve time reading {dt} in zone {name:?}: {e}"))
}
async fn sleep(ms: i64) -> Result<(), String> {
	if !(0..=MAX_SLEEP_MS).contains(&ms) {
		return Err(format!(
			"cannot sleep for {ms} milliseconds: expected 0..={MAX_SLEEP_MS}"
		));
	}
	tokio::time::sleep(Duration::from_millis(ms as u64)).await;
	Ok(())
}
pub fn install(context: &mut Context) -> crate::Result<Vec<HostFunction>> {
	let mut module = Module::with_crate("time")?;
	module.function("now_ms", now_ms).build()?;
	module.function("monotonic_ms", monotonic_ms).build()?;
	module.function("format", format).build()?;
	module.function("rfc3339", rfc3339).build()?;
	module.function("parse", parse).build()?;
	module.function("parts", parts).build()?;
	module.function("from_parts", from_parts).build()?;
	module.function("sleep", sleep).build()?;
	context.install(module)?;
	Ok([
		("now_ms","now_ms() -> i64: wall-clock milliseconds since Unix epoch, floored; not an elapsed-time clock"),
		("monotonic_ms","monotonic_ms() -> i64: milliseconds since first call in this process; never decreases; not an epoch timestamp"),
		("format","format(ms, pattern, zone) -> Result<String>: Jiff strftime dialect; fallible, precision chosen by the pattern"),
		("rfc3339","rfc3339(ms, zone) -> Result<String>: three fractional digits; refuses negative years and offsets outside whole minutes in ±23:59"),
		("parse","parse(text) -> Result<i64>: floor milliseconds; offset or Z required; leap seconds refused, zone annotations checked (Z preserves the instant)"),
		("parts","parts(ms, zone) -> Result<Object>: calendar fields, ISO weekday, offset_seconds and resolved zone"),
		("from_parts","from_parts(object, zone) -> Result<i64>: year/month/day required; clock fields default to zero; gaps refused, folds require a valid offset_seconds; weekday and zone fields are informational"),
		("sleep","sleep(ms).await -> Result<()>: 0..68719476735 ms; inert until awaited, interruptible while pending"),
	].into_iter().map(|(name,doc)|HostFunction{path:format!("time::{name}"),doc}).collect())
}
#[cfg(test)]
mod tests {
	use super::*;
	#[test]
	fn conversion_floors_and_keeps_both_endpoints() {
		for n in [
			MIN_MS,
			MIN_MS + 1,
			-1001,
			-1000,
			-999,
			-1,
			0,
			1,
			999,
			1000,
			MAX_MS - 999,
			MAX_MS,
		] {
			assert_eq!(milliseconds(timestamp(n).unwrap()), n);
		}
		for n in [MIN_MS - 1, MAX_MS + 1, i64::MIN, i64::MAX] {
			assert!(timestamp(n).unwrap_err().contains(&n.to_string()));
		}
		let sub: Timestamp = "1969-12-31T23:59:59.999999999Z".parse().unwrap();
		assert_eq!(sub.as_millisecond(), 0);
		assert_eq!(milliseconds(sub), -1);
	}
	#[test]
	fn exact_surface_compiles() {
		let mut c = Context::with_default_modules().unwrap();
		let names = install(&mut c).unwrap();
		assert_eq!(
			names.iter().map(|n| n.path.as_str()).collect::<Vec<_>>(),
			[
				"time::now_ms",
				"time::monotonic_ms",
				"time::format",
				"time::rfc3339",
				"time::parse",
				"time::parts",
				"time::from_parts",
				"time::sleep"
			]
		);
		for n in names {
			crate::compile(&c, &format!("pub fn main() {{ {} }}", n.path)).unwrap();
		}
	}
}
