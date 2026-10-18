//! The proving host. [`pipeline`] drives batches through the SP1 zkVM; see its module docs.

#[cfg(test)]
mod tests;

pub mod memory_monitor;
pub mod observability;
pub mod pipeline;
pub mod util;
