use wire_type::wire_type;

#[wire_type(foo)]
#[repr(C)]
struct Pair {
    a: u32,
    b: u32,
}

fn main() {}
