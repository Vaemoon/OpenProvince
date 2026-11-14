# OpenProvince desktop interface

A quiet Windows resource editor with a classic full-width menu, compact commands, a resizable resource list and a permanent inspector. Preserve the existing archive and texture workflows.

Use theme.rs as the source of truth: graphite chrome (#1e1e20), panels (#252528), primary text (#e2e4e8), secondary text (#9c9ea6), restrained blue selection. Segoe UI, 13 px body and 24 px controls. Use bundled Bootstrap SVG icons; do not substitute emoji. Group content with spacing, not nested outlined cards.

The launcher follows the VS Code run control: a small green play triangle, the label Играть and a separate compact dropdown chevron. Keep launch variants and executable selection in that menu. No green container or mod-count badge.

Modifications use a narrow searchable library on the left and an unboxed detail inspector on the right. Keep category filtering beside search. Installation and applying the selected set are the common commands; folder import, restoring originals and folder navigation belong in Ещё. Explain selection separately from what is applied to game archives. Show conflicts and operation errors as readable text. Confirm deletion. Preserve search focus when clearing text.

The catalog lists actual local .opmod/.zip packages using their real manifest metadata. Never invent downloadable packages or report a successful installation by creating a manifest alone. Preview images retain their aspect ratio.

Visual checks: debug-only visual_check.rs captures the application's own framebuffer without activating its window. Verify 1280x820 and 900x560, search with no results, deleting the last search character, and the launcher dropdown. It is excluded from release builds.


The game launcher is available only with the mods feature. The resource-only archiver has no launch controls. Run/stop share a neutral compact group. Track only verified executables from the selected installation; show stopping state, reject repeated stops and never request elevation for an already exited process. Poll process metadata outside the UI thread. Disable duplicate launches while the game is running.

UI tokens stay in theme.rs, icon assets and rendering in theme/icons.rs, reusable controls in theme/controls.rs. The mods area separates state, installed-library UI and package-catalog UI. Game process discovery, launch actions and launcher presentation are separate modules. First-party source contains no comments; documentation belongs in Markdown files. Bootstrap Icons is the sole UI icon package; brand links without a matching Bootstrap mark use a generic icon from that package.
