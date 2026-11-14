#[cfg(not(target_os = "zkvm"))]
pub mod public {
    pub use crate::window::{WINDOW_SIZE, Window, WindowState, WindowValues};
}

#[cfg(not(target_os = "zkvm"))]
pub(crate) mod local {}

#[cfg(target_os = "zkvm")]
pub mod public {
    pub use crate::window::{Window, WindowState, WindowValues};
}

#[cfg(target_os = "zkvm")]
pub(crate) mod local {
    pub(crate) use crate::window::WINDOW_SIZE;
}
