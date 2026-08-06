# Regular-mesh extrapolation

Regular-mesh extrapolation is an explicit, provenance-preserving repair tool for
**regular TCT grids only**. It fills eligible `NA` values from the canonical
ternary lattice's three mesh-line families. It never runs automatically during
projection, never changes `CO`, and does not provide an irregular-grid
algorithm.

## Algorithm

For every synchronous layer, each eligible `NA` vertex examines its six stable,
directed lattice rays. A ray walks inward through contiguous finite calculated
or prior-layer `EX` samples; the first unavailable cell is a barrier. The core
uses the selected existing one-dimensional Akima, Makima, PCHIP, or Steffen
cubic implementation to evaluate exactly one mesh step beyond the nearest
support endpoint.

All candidates in a layer use the same immutable preceding snapshot. Accepted
values are committed only after the complete layer has been evaluated, making
the result independent of vertex traversal order. Directional estimates are
sorted by stable direction ID; one estimate is retained only when allowed by
the support threshold, two are averaged, and three or more use a deterministic
trimmed/median rule. Spread, scalar-bound, support, and finite-value guards may
reject a candidate without clamping it.

## Provenance and persistence

An accepted value is serialized as:

```text
EX[layer,method,support,spread]=value
```

For example:

```text
EX[1,steffen,2,3.25]=812.340000
```

`EX` means **Extrapolated**, not calculated. The model stores the layer,
cubic method, directional support count, and estimate spread. Projection source
preparation may consume EX as a finite value, but diagnostics retain that
provenance. Any ordinary edit or paste to a field clears all EX cells in that
field to `NA` in the same transaction, preventing stale continuations.

Legacy `NE` input is accepted as a compatibility alias for `NA`; it is
normalized on read and never serialized. `CO` remains distinct and is
ineligible for automatic extrapolation.

## CLI and Qt workflow

Preview first:

```bash
cargo run -p ternary-contours-cli -- extrapolate-mesh input.tct \
  --grid regular --field Lime.T --method steffen --max-layers 1 --preview
```

Materialize only the previewed values:

```bash
cargo run -p ternary-contours-cli -- extrapolate-mesh input.tct \
  --grid regular --field Lime.T --method steffen --max-layers 1 \
  --output healed.tct
```

In the Qt Data tab use **Grid → Extrapolate Missing Values…**, choose a field
or all fields, preview, then materialize. The Qt application delegates both
preview and materialization to Rust; it does not compute directional estimates.
An irregular grid presents the explicit regular-grid-only explanation.

## Tracing

The opt-in numerical trace records mesh extrapolation start/completion, each
layer, directional support decision, accepted/rejected direction, and
accepted/rejected vertex. Tracing is observation-only: trace-on and trace-off
produce identical extrapolated values and layer order.