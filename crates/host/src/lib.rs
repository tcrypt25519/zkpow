//! The proving host. [`pipeline`] drives batches through the SP1 zkVM; see its module docs.

#[cfg(test)]
mod tests;

pub mod memory_monitor;
pub mod observability;
pub mod pipeline;
pub mod util;

#[cfg(feature = "memory-diagnostics")]
#[global_allocator]
pub static MEMORY_ALLOCATOR: memuse::Allocator<std::alloc::System> = memuse::Allocator::new();

pub use memuse;
