use wiretype_macro::wiretype;

#[wiretype(foo)]
#[repr(C)]
struct Pair {
    a: u32,
    b: u32,
}

fn main() {}
