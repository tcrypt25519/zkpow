---
id: idea-vast-ai-live-verification
created: 2026-11-04
updated: 2026-11-04
---

# Live Vast.ai RTX 4090 Verification Batches

Launch an RTX 4090 instance on Vast.ai using `./scripts/vast/launch-instance.sh <offer_id>`. Copy `env.vast` across and execute `bootstrap.sh` (tests auto-pull of `headers.db.tar.zst` from Backblaze B2). Run 5-10 batches of 2,016 headers to confirm single-shard prove time (~48s), flat resident memory (~4-6 GB), and zero GPU VRAM spikes. Test interrupt-and-resume from SQLite ledger and continuous background offsite sync to Backblaze B2 (`s3://zkpow-proofs/`).
