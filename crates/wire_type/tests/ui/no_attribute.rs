#[derive(bytemuck::Pod, bytemuck::Zeroable)]
#[repr(C)]
#[derive(Clone, Copy)]
struct Plain {
    a: u32,
    b: u32,
}

fn main() {
    let x = Plain { a: 1, b: 2 };
    let _ = zkpow_types::as_wire_bytes(&x);
}
