# Critique & Distill Reference

## 1. Design Specificity & Squint Test
- Blur detail mentally: can you immediately distinguish primary workspace, secondary list/navigation, active toolbars, and semantic status?
- If the layout looks like generic developer boilerplate (flat grey boxes, unstyled text tabs, ungrouped buttons), assign concrete surface contrast and spatial rhythm.

## 2. Usability Heuristics for Power Tools
- **Visibility of system status:** Dirty files, pending archive edits, loading/busy operations, and active filters must be visible in-place without forcing the user to open another panel.
- **Match real-world modding mental models:** 3D viewports use LMB orbit, RMB/MMB/Shift+LMB pan, Wheel zoom, `R` reset, camera presets. Texture viewers use Wheel zoom, drag pan, double-click 100%/fit, checkerboard/black/white backdrop toggle.
- **Recognition rather than recall:** Surface key shortcuts (`Ctrl+O`, `Ctrl+S`, `Ctrl+F`, `F3`, `Del`, `R`, `Esc`) in tooltips and compact hints rather than burying them only in Help.
- **Flexibility and efficiency of use:** Power users navigate lists with `Up/Down`, jump with `Ctrl+F`, switch paired `.dff <-> .txd` assets in 1 click, and right-click rows for common actions.

## 3. Distill Checklist
- Remove duplicated controls across top menu + panel header when a single unified toolbar/menu owns the job.
- Replace bulky multi-line status panels that steal permanent vertical height with compact badges + on-demand popovers/drawers.
- Keep labels short and scannable ("Извлечь…", "Заменить…", "Вписать") with active verbs.
- Limit type-badge color coding to calm, desaturated pills so text remains effortless to read.
