# Deterministic numerical trace schema

`ternary-contours` provides an opt-in, observation-only numerical trace. It is
for reproducing and diagnosing one exact numerical request; it is not a TCT
section, a renderer log, a cache key, or a document preference.

## Enabling a trace

The core API is always available and defaults to `NumericalTraceLevel::Off`.
The CLI JSON Lines writer is optional:

```powershell
cargo run -p ternary-contours-cli --features trace -- `
  trace-projection tools/ternary-contours-cli/fixtures/interior-invariant.tct `
  --output target/interior.trace.jsonl --level decisions --max-events 500000
```

Use `analyze-trace` for structural checks:

```powershell
cargo run -p ternary-contours-cli --features trace -- `
  analyze-trace target/interior.trace.jsonl
```

The Qt application exposes the same request in **Settings → Developer
diagnostics**. The dialog only tells Rust what to observe on a subsequent
Viewer calculation. It does not write a TCT preference, change calculation
options, mark the document dirty, or advance its numerical-options revision.

## Envelope and ordering

Every line is one JSON object serializing `NumericalTraceEvent`:

```text
schema_version  u32, currently 1
sequence        monotonically increasing u64 from 0 for one calculation
stage           snake_case NumericalTraceStage
payload         typed event payload
```

There are no timestamps, thread IDs, pointer values, or random identifiers.
Semantic compositions are always `[A, B, C]`, use the dataset component order,
and sum to one within the numerical tolerance. Source rows, sampling triangles,
path IDs, invariant IDs, and univariant IDs are zero-based core identifiers.
User interfaces may display rows one-based, but never change the trace value.
All finite scalars are written as JSON numbers with serde's round-trip-capable
representation.

The payload vocabulary is a stable set of snake-case
`NumericalTraceEventKind` values. A `Decision` payload has a typed sparse
`detail` object: inapplicable values are omitted as `null`, never represented
with NaN or sentinel temperatures.

## Levels and filters

- `off`: no events and no event-only allocations.
- `summary`: run lifecycle and deterministic stage summaries.
- `decisions`: source classification, topology outcomes, path decisions, and
  contour assembly summaries.
- `iterations`: reserved for per-iteration root, refinement, and local
  interpolation observations as they are enabled by individual kernels.

`NumericalTraceConfig` supports an event, binary-boundary, phase,
phase-pair, triangle, and semantic-composition-region filter. Filters only
remove observations; they never change numerical traversal.

When the configured event cap is reached, the stream contains exactly one
`trace_truncated` event. A terminal `run_completed` or `run_failed` remains
recorded afterward so an accepted trace always has a lifecycle end event.

## Event stages

| Stage | Representative event names | Important context |
| --- | --- | --- |
| `run` | `run_started`, `run_completed`, `run_failed`, `trace_truncated` | crate/version, phases, interpolation, revisions/request ID, final counts |
| `source_preparation` | `phase_field_located`, `source_coverage_computed`, `partial_source_prepared` | phase, grid/property identity, calculated/NE/CO/NA counts |
| `interpolation` | `interpolation_triangle_located`, `one_sided_cubic_selected`, `linear_fallback_selected` | semantic composition, triangle/source rows, local barycentric coordinates |
| `stable_selection` | `phase_value_evaluated`, `stable_winner_selected`, `stable_tie_detected` | candidates, defined values, undefined reasons |
| `binary_boundary` | `binary_transition_bracketed`, `binary_root_iteration`, `binary_invariant_emitted` | AB/BC/CA parameter bracket, pair residual, stable phase set |
| `interior_invariant` | `interior_solve_started`, `interior_invariant_accepted` | sampling triangle, phase set, composition, residual |
| `univariant` | `pending_end_created`, `univariant_triangle_entered`, `univariant_trace_completed` | pending end, phase pair, node/path IDs, termination |
| `contour` | `contour_level_started`, `contour_path_completed`, `contour_level_completed` | level, phase, path/junction identity, open/closed state |
| `regularization` | `regularization_started`, `regularization_projection_backtracked`, `regularization_completed` | path identity, spacing, accepted/rejected projection reason |
| `error` | `invalid_root_bracket`, `unresolved_pending_ends`, `regularization_non_convergence` | typed contextual reason accompanying the public error |

The event-name enum includes the complete vocabulary above even where a
specific kernel currently exposes its deterministic summary rather than every
inner iteration. Additive per-kernel instrumentation must reuse these names;
it must not introduce `println!` diagnostics or GUI-side reconstructed events.

## Reproducible workflow

1. Retain the exact TCT, complete projection options, crate commit, and
   projection summary.
2. Run a bounded `decisions` trace.
3. Inspect `analyze-trace` for lifecycle, sequence, truncation, and topology
   warnings.
4. Narrow with a phase, pair, boundary, triangle, or event filter.
5. Use `iterations` only for the suspicious kernel and retain the resulting
   JSON Lines file with the reproduction.

A trace-output error is separate from numerical success. The calculation result
remains valid and the Viewer retains its last accepted projection; the output
status states that the trace could not be written.