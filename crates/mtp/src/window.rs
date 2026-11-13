use zkpow_types::BlockTimestamp;

use crate::{Cold, Ring, RingValues, Warm};

/// Number of previous block timestamps kept for median-time-past and ordered-window checks.
pub const WINDOW_SIZE: usize = 11;

/// The consensus timestamp window.
pub type Window<F = Warm> = Ring<{ WINDOW_SIZE }, F>;
pub type WindowValues = RingValues<{ WINDOW_SIZE }>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowState {
    Cold(Window<Cold>),
    Warm(Window<Warm>),
}

impl crate::WindowState {
    pub fn from_height(values: WindowValues, height: u32) -> Self {
        if height < (WINDOW_SIZE - 1) as u32 {
            let mut cold = Window::<Cold>::new(values[0]);
            for &val in &values[1..=height as usize] {
                cold.push(val);
            }
            Self::Cold(cold)
        } else {
            Self::Warm(Window::<Warm>::from_height(values, height))
        }
    }

    pub fn from_timestamps(start_height: u32, timestamps: impl IntoIterator<Item = BlockTimestamp>) -> Option<Self> {
        let mut values = [BlockTimestamp::new(0); WINDOW_SIZE];
        let mut slot = start_height as usize % WINDOW_SIZE;
        let mut height = None;
        for (offset, timestamp) in timestamps.into_iter().enumerate() {
            values[slot] = timestamp;
            slot += 1;
            if slot == WINDOW_SIZE {
                slot = 0;
            }
            height = Some(start_height + offset as u32);
        }
        height.map(|height| Self::from_height(values, height))
    }

    pub const fn values(&self) -> WindowValues {
        match self {
            Self::Cold(ring) => ring.values(),
            Self::Warm(ring) => ring.values(),
        }
    }

    pub const fn newest(&self) -> BlockTimestamp {
        match self {
            Self::Cold(ring) => ring.newest(),
            Self::Warm(ring) => ring.newest(),
        }
    }

    pub const fn requires_median_hint(&self) -> bool {
        match self {
            Self::Cold(_) => false,
            Self::Warm(ring) => ring.requires_median_hint(),
        }
    }
}
