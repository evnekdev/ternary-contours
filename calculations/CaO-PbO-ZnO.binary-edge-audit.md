# CaO–PbO–ZnO binary-edge audit

Linear source interpolation; roots are isolated exactly on connected finite source-edge segments.

## CaO–PbO (Ab): Lime.T − PbO.T

| finite interval | D(start) | D(end) | min D | max D | sign change | root | classification |
| --- | ---: | ---: | ---: | ---: | --- | --- | --- |
| [0.600000000000, 0.900000000000] | 1596.450000000 | 13.220000000 | 13.220000000 | 1596.450000000 | no | — | not confirmed |

**NOT CONFIRMED:** finite overlap exists but no sign change was found. Roots: 0.

## PbO–ZnO (Bc): PbO.T − ZnO.T

| finite interval | D(start) | D(end) | min D | max D | sign change | root | classification |
| --- | ---: | ---: | ---: | ---: | --- | --- | --- |
| [0.100000000000, 0.900000000000] | -183.790000000 | -1566.120000000 | -1566.120000000 | -183.790000000 | no | — | not confirmed |

**NOT CONFIRMED:** finite overlap exists but no sign change was found. Roots: 0.

## ZnO–CaO (Ca): ZnO.T − Lime.T

| finite interval | D(start) | D(end) | min D | max D | sign change | root | classification |
| --- | ---: | ---: | ---: | ---: | --- | --- | --- |
| [0.000000000000, 0.500000000000] | 537.530000000 | -1009.720000000 | -1009.720000000 | 537.530000000 | yes | 0.347741780302 | stable binary invariant candidate |

**CONFIRMED:** a continuous finite interval exists and D changes sign. Roots: 1.


## Current binary scanner comparison

Qt-equivalent Linear options (sampling 20, regularization enabled) produced 1 binary invariant(s) and 2 typed unavailable transition(s). The independent raw-edge audit agrees: AB and BC contain no finite sign-changing overlap; CA contains the sole stable root.
