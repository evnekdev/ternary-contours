# Window behavior

The native window receives an initial logical size only at startup. Frame updates never issue inner-size or outer-position commands. DPI changes update physical-rendering provenance only; they do not resize or reposition the native window.
