# Notice

mxm-ara — Copyright (C) 2026 mxm. MIT, as the rest of mxm-kit (the repository's `LICENSE`); moved
here from mxm-fx-pitch on 2026-11-23 (the owner: "yes, move mxm-ara to mxm-kit under MIT").

## Third-party code in it

- **ARA API 2.3** (Celemony's Audio Random Access), `ara_api/ARAInterface.h`, vendored unchanged
  from the ARA SDK 2.3 (tag `releases/2.3.0`, commit `0005c532bb03eb210e7bb490329fc6d7ba1792c7`):
  Copyright (c) 2012-2025, Celemony Software GmbH, All Rights Reserved; developed in cooperation
  with PreSonus Software Ltd. Licensed under the Apache License, Version 2.0
  (`ara_api/LICENSE.txt`); the SDK's notice is `ara_api/NOTICE.txt`.
- **`src/sys/x86_64.rs` and `src/sys/aarch64.rs`** are generated from that header by bindgen
  (`tools/gen-bindings.sh`), and so are a derivative of it under the same Apache License 2.0.

The rest of the crate is written in Rust from ARA's API documentation and its C++ reference
library (ARA_Library) and examples, read for their semantics; none of their code is copied.

ARA and Audio Random Access are trademarks of Celemony Software GmbH.
