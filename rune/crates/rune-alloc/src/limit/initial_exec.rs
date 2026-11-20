//! A startup-linked ELF image has a loader-assigned offset in each thread.
//! Only this allocation-budget word uses initial-exec TLS. The hidden symbol
//! binds the GOT relocation to this image and FS addresses the calling thread.
use core::arch::{asm, global_asm};

global_asm!(
	".pushsection .tdata.rnx_memory,\"awT\",@progbits",
	".balign 8",
	".globl rnx_allocator_memory",
	".hidden rnx_allocator_memory",
	".type rnx_allocator_memory,@tls_object",
	"rnx_allocator_memory:",
	".quad -1",
	".size rnx_allocator_memory,8",
	".popsection",
);

#[inline(always)]
fn offset() -> usize {
	let value;
	unsafe {
		asm!("mov {v}, qword ptr [rip + rnx_allocator_memory@GOTTPOFF]",
        v = out(reg) value, options(pure, readonly, nostack, preserves_flags))
	}
	value
}

#[inline(always)]
fn read(off: usize) -> usize {
	let value;
	unsafe {
		asm!("mov {v}, qword ptr fs:[{off}]", v = out(reg) value,
        off = in(reg) off, options(readonly, nostack, preserves_flags))
	}
	value
}

#[inline(always)]
fn write(off: usize, v: usize) {
	unsafe {
		asm!("mov qword ptr fs:[{off}], {v}",
        off = in(reg) off, v = in(reg) v, options(nostack, preserves_flags))
	}
}

pub(super) fn rune_memory_take(amount: usize) -> bool {
	let off = offset();
	let v = read(off);
	if v >= amount {
		write(off, v.wrapping_sub(amount));
		true
	} else {
		false
	}
}

pub(super) fn rune_memory_release(amount: usize) {
	let off = offset();
	write(off, read(off).saturating_add(amount));
}

pub(super) fn rune_memory_get() -> usize {
	read(offset())
}

pub(super) fn rune_memory_replace(value: usize) -> usize {
	let off = offset();
	let old = read(off);
	write(off, value);
	old
}
