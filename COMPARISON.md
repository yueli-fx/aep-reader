# Research and comparison boundaries

Format lineage is documented in [PROVENANCE.md](PROVENANCE.md). The public
repository contains the Rust reader, synthetic tests and optional local-file
checks. Private reference API inventories, Go comparison harnesses, full local
reports and original AEP assets are not distributed.

Reference-reader agreement is only a diagnostic aid, not proof of correctness.
Tests at the public byte boundary cover malformed inputs and explicit semantic
expectations. Local real-file and EffectCraft checks are summarized in
[VALIDATION.md](VALIDATION.md); their original inputs are not included.
