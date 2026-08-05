# Qt UI translation

All user-visible XML strings use normal `<string>` elements and must not set
`notr="true"`. The core XML scanner rejects that opt-out. Static Designer
strings are therefore available to Qt's standard `lupdate`/translation-source
workflow; adapter code uses `tr()` for runtime text.

Stable `objectName` values, generated Rust IDs, contract IDs, paths, and
machine-readable state tokens are never translated. User-facing status,
validation, menu, dialog, tooltip, and accessible-description strings are
translated where their owning framework supports it.

When the first distributable Qt target is added, its CMake build will add an
`lupdate`/`lrelease` target and package compiled translation resources.