#[cfg(test)]
mod tests;

#[cfg(feature = "CUDA")]
pub(crate) mod cuda_env;
pub mod memory_monitor;
pub mod observability;
pub mod pipeline;
pub mod util;

#[cfg(feature = "memory-diagnostics")]
#[global_allocator]
pub static MEMORY_ALLOCATOR: memuse::Allocator<std::alloc::System> = memuse::Allocator::new();

pub use memuse;
