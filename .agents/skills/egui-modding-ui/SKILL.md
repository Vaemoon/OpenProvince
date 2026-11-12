---
name: egui-modding-ui
description: >-
  Use this skill when designing, refactoring, or reviewing Rust egui / eframe
  desktop interfaces for game modding tools, IMG/TXD/DFF browsers, 3D model
  inspectors, and texture editors (inspired by OpenIV, Magic.TXD, Blender,
  and Unreal Content Browser).
---

# egui Desktop Modding UI Engineering

Follow these rules whenever modifying `OpenProvince` UI modules (`theme.rs`, `ui.rs`, `gpu.rs`, `texture_editor.rs`, `main.rs`).

## 1. Visual Architecture in `egui`
- Centralize all colors, strokes, radii, spacing, and typography tokens in `src/theme.rs` and sync them with `DESIGN.md`.
- Maintain strict surface layering:
  - **Chrome (`BG_CHROME`)**: top menu bar, archive tabs, status footer.
  - **Panel (`BG_PANEL`)**: resource browser table, texture list.
  - **Surface (`BG_SURFACE`) / Elevated (`BG_ELEVATED`)**: toolbars, popovers, inspector headers, hovered items.
  - **Viewport / Canvas (`BG_VIEWPORT`)**: deepest neutral backdrop for 3D DFF and 2D TXD inspection.
- Give inputs and secondary buttons crisp `1px` subtle borders (`BORDER_SUBTLE`) so widgets remain distinct in dark mode without heavy shadows.

## 2. Modder Workflow Ergonomics
- **Archive Workspace:** Show open `.img` tabs as proper selectable tabs with file count and dirty `*` indicator; provide 1-click `+` add archive and clear feedback if edits lock tab switching.
- **Fast Filtering:** Support `Ctrl+F` instant search focus, `Esc` clear, and 1-click asset type chips (`Все`, `DFF`, `TXD`, `Изм.`).
- **Paired Asset Navigation:** When viewing a `.dff`, offer 1-click jump to its matching `.txd` (and vice versa) if present in open archives.
- **3D DFF Viewport (`gpu.rs`):** Keep render toggles (`Текстуры`, `Каркас`, `Прозрачность`, `Автоповорот`), camera presets (`3/4`, `Спереди`, `Сбоку`, `Сверху`, `Сброс`), body color swatches, component checklist, and viewport backdrop selector directly reachable. Support LMB orbit, RMB/MMB/Shift+LMB pan, wheel zoom, and `R`/double-click camera reset.
- **TXD Texture Editor (`texture_editor.rs`):** Expose Add, Replace, Delete, Rename, Undo (`Ctrl+Z`), Redo (`Ctrl+Y`), and Export cleanly; draw thumbnails over mini checkerboards; support wheel zoom, drag pan, double-click Fit/100%, channel isolation (`RGBA`/`RGB`/`Alpha`), and backdrop switching (`Шахматка`/`Чёрный`/`Белый`/`Серый`).
- **Context-Aware Drag & Drop:** Drop `.img` -> open archives; drop images while TXD editor is open -> import/replace texture in TXD; otherwise stage files into active IMG.

## 3. Code & Test Invariants
- Never break focus stability or height contracts tested in `ui::search_tests`.
- Keep UI drawing methods modular and readable with shared token/pill helpers instead of duplicated styling blocks.
- Verify offline builds and tests via `cargo test --workspace --offline`.
