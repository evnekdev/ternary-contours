# Qt deployment

Package only the Qt 6 modules actually linked by the final application. The
prototype uses Core, Gui, Widgets, and Concurrent; later exports may add Svg.

On Windows, run the Qt SDK's `windeployqt` against the release executable after
linking the Rust static library, then test on a clean VM. On Linux, distribute
through the target package manager, an AppImage/Flatpak policy, or an
application bundle with compatible Qt runtime libraries. Always include the
licenses and notices required by the selected Qt licensing path and Qt's
third-party-code inventory.

The Rust numerical core remains an ordinary Rust dependency; no Qt dependency
is added to the publishable root numerical package.