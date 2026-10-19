#[derive(bytemuck::Pod, bytemuck::Zeroable)]
#[repr(C)]
#[derive(Clone, Copy)]
struct Plain {
    a: u32,
    b: u32,
}

fn main() {
    let _ = <Plain as zkpow_types::WireBytes>::SIZE;
}
