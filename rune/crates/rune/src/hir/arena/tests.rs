use super::Arena;

#[test]
fn basic() {
	let arena = Arena::new();

	let first = arena.alloc(1u32).unwrap();
	let second = arena.alloc(2u32).unwrap();

	assert_eq!(first, &1);
	assert_eq!(second, &2);
}

#[test]
fn slices() {
	let arena = Arena::new();

	let hello = arena.alloc_bytes(b"hello").unwrap();
	let world = arena.alloc_bytes(b"world").unwrap();

	assert_eq!(hello, b"hello");
	assert_eq!(world, b"world");
}

mod empty_literals;

#[test]
fn zero_size_addresses_and_storage() {
	use core::alloc::Layout;
	let arena = Arena::new();
	for alignment in [1, 64, 4096] {
		let pointer = arena
			.alloc_raw(Layout::from_size_align(0, alignment).unwrap())
			.unwrap();
		assert_eq!(pointer.as_ptr().addr(), alignment);
		assert_eq!(pointer.as_ptr().addr() % alignment, 0);
	}
	assert!(arena.chunks.borrow().is_empty());
	assert!(arena.start.get().is_null());
	assert!(arena.end.get().is_null());
	let first = arena.alloc_bytes(b"").unwrap();
	let second = arena.alloc_bytes(b"").unwrap();
	assert!(core::ptr::eq(first, second));
	assert_eq!(arena.alloc_str("").unwrap(), "");
	#[repr(align(64))]
	struct Aligned;
	let value = arena.alloc(Aligned).unwrap();
	assert_eq!((value as *mut Aligned).addr() % 64, 0);
	let mut iter = arena.alloc_iter::<Aligned>(2).unwrap();
	assert!(iter.write(Aligned).is_ok());
	assert!(iter.write(Aligned).is_ok());
	assert_eq!(iter.finish().len(), 2);
	assert!(arena.chunks.borrow().is_empty());
}
