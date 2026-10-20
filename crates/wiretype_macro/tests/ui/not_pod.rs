use wiretype::wiretype;

#[wiretype]
#[derive(Clone, Copy)]
struct NotReprC {
    a: u32,
    b: u16,
}

fn main() {}
