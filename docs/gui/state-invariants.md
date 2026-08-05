# State invariants

- A clean document has no outstanding draft edits.
- A current projection has a projection and no pending calculation.
- Current queries have no pending query batch.
- A dialog cannot overlap conflicting document I/O.
- A visible panel has non-zero reachable width.
- Async completion requires matching request and input revisions.
