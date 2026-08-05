# GUI contract testing

Reducer tests run without GUI, native dialogs, filesystem, or workers. Effect interfaces are small enough for fakes. Generated documentation is compared with checked-in files. Geometry tests exercise DPI transitions without a physical multi-monitor test runner.

## Coverage inventory

```text
Interactive elements:                 72
Registered contracts:                 72 / 72
Fully contract-driven elements:       14 / 72
Public state categories:              20 / 20
Action kinds with reducer coverage:   30 / 30
Effect kinds with executor coverage:  12 / 12
Layout contracts:                     72 / 72
```

The migration count is deliberately reported separately: declared entries retain their contracts and documentation while their legacy rendering paths are incrementally moved behind contract-aware wrappers.
