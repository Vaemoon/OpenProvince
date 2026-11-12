# Polish & Clarify Reference

## Visual & Spatial Polish
- **Spacing scale:** Strictly stick to `4, 6, 8, 12, 16, 24` px increments.
- **Unified control heights:** Buttons, search fields, filter pills, and combo boxes inside the same toolbar share a single baseline height (`26.0` or `28.0` px).
- **Typography & numbers:** Use tabular / monospace sizing for byte counts, triangle/vertex counts, zoom percentages, and hex offsets. Never use `ALL CAPS` sentences for normal labels.
- **Icons:** Use a single coherent Lucide stroke icon family (`1.5px` stroke, `14–16px` size), tinted with semantic foreground/muted colors. Align icon + label vertically centered.

## Copy & Microcopy
- Prefer clear, idiomatic Russian tool terminology without machine-translated stiffness.
- Empty states must guide the next action (e.g., open archive, scan folder, drop files) instead of printing a dead-end single sentence.
- Error & lock states must tell the user *why* an action is unavailable and *how* to unblock it (e.g., saving/discarding pending edits in the active archive before editing another).

## Edge Cases (`break` check)
- Long file names (`very_long_vehicle_paintjob_name.txd`) truncate cleanly with ellipsis + tooltip containing full name.
- Narrow panels (`900px` minimum window width) wrap or adapt without overlapping buttons or hiding primary actions.
- High-DPI / mouse-wheel zooming does not hijack scrolling when hovering interactive canvases.
