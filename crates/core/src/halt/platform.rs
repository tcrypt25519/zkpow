extern crate std;

#[inline(never)]
pub fn halt_invalid_hint() -> ! {
    panic!("halt: invalid hint (3)")
}

#[doc(hidden)]
#[cold]
#[inline(never)]
pub fn __invalid_hint_fmt(args: core::fmt::Arguments<'_>) -> ! {
    std::eprintln!("invalid prover hint: {args}");
    halt_invalid_hint()
}
