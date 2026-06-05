# State V2

## Public Input / Output

| Byte Offset | Field       | Type      |
|-------------|-------------|-----------|
| [0]         | version     | u8        |\
| [1]         | status      | u8        |  State Header
| [2..3]      | reserved    | u16       |/
| [4..7]      | height      | u32       |\-5
| [8..39]     | anchor_hash | 32 bytes  |  State Body
| [40..71]    | chainwork   | 32 bytes  |  (Core Data)
| [72..103]   | block_hash  | 32 bytes  |/
| [104..135]  | witness_cmt | 32 bytes  |\ State
| [136..167]  | inner_vk    | 32 bytes  |/ Integriry

## Processing

The verifier;

- Reads `versiom` and ensure it under stands Integriry
- Reads `status` and ensure it's 0 (success)
  - If `status` is 1, the last attempted state was invalid and the State Body is still the tip of the chain
- Reads `height` and ensure it's an expected height.
-- Reads `anchor_hash] and ensure it's the expected anchor hash for the given height
