# State dependencies

Dataset edit -> dataset revision -> interpolator/query stale -> projection stale -> texture and hit geometry stale.

Interpolation settings -> settings revision -> registered queries and projection recalculation.

View transform -> hit geometry rebuild only.
