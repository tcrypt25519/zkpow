use wire_type::wire_type;

#[wire_type]
#[derive(Clone, Copy)]
struct NotReprC {
    a: u32,
    b: u16,
}

fn main() {}
