//! Record 0143: can native code advance a script iterable through Rune
//! 0.14.2's public API? Expected: no. `Iterator::next` and
//! `Value::protocol_next` are crate-private, so this must not compile.
pub fn count(v: rune::Value) -> usize {
	let mut it: rune::runtime::Iterator = rune::from_value(v).unwrap();
	let mut n = 0;
	while let rune::runtime::VmResult::Ok(Some(_)) = it.next() {
		n += 1;
	}
	n
}

pub fn count_by_protocol(v: rune::Value) -> usize {
	let mut n = 0;
	while let rune::runtime::VmResult::Ok(Some(_)) = v.protocol_next() {
		n += 1;
	}
	n
}
