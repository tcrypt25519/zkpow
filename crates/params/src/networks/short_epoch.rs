/// A synthetic network that differs from [`Mainnet`] only in `epoch_length`, so tests can cross a
/// retarget boundary cheaply and prove the transition reads the boundary from the parameters
/// rather than assuming 2016.
///
/// Not Bitcoin's regtest: real regtest shares mainnet's epoch length, which is exactly what makes
/// it useless here. Feature-gated so it cannot be reached from a production build.
use super::*;

pub struct ShortEpoch;

impl Consensus for ShortEpoch {
    const PARAMS: ConsensusParams = ConsensusParams {
        // Comfortably past the cold-start window, so a boundary lands in the warm path.
        epoch_length: 32,
        ..MAINNET_PARAMS
    };
}
