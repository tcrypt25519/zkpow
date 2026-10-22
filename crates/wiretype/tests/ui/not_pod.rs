use wiretype::wiretype;

#[wiretype]
#[derive(Clone)]
struct NotReprC {
    a: u32,
    b: u16,
}

fn main() {}
