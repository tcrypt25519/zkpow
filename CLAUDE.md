# zkpow26

Zero-knowledge proof system for Bitcoin header chain validation using SP1 zkVM.
Proves that a batch of block headers are valid (PoW, chain linkage, difficulty
retargeting, median time past) and can be recursively chained to extend a proof
from any trusted starting point.

**Read `AGENT_RULES.md` before making any code changes.**

See `AGENTS.md` for the full operational guide (project structure, proving
workflows, environment variables, debugging, database schema).
