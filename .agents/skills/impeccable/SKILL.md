---
name: impeccable
description: >-
  Use this skill when auditing, critiquing, simplifying, or polishing any UI
  surface. Combines structured UX heuristics, design-system token alignment,
  visual noise distillation, and final craft passes inspired by pbakaus/impeccable
  and agentsorg/design-engineering.
---

# Impeccable UI Craft (Desktop & Tooling Edition)

Use this workflow when refining an existing interface rather than rebuilding blindly.

## Core Loop

1. **Document first (`DESIGN.md`):** Before editing UI code, read or create `DESIGN.md` with exact palette layers, spacing scale, typography roles, radii, and semantic states. Never introduce one-off colors or arbitrary padding.
2. **Critique (`critique`):** Evaluate the whole user journey using [critique-and-distill.md](./references/critique-and-distill.md). Identify friction, hidden tools, weak visual hierarchy, and missing domain ergonomics. Record findings as a concrete `Before | After | Why` table.
3. **Distill (`distill`):** Strip clutter without removing capabilities. Consolidate duplicated menus, eliminate nested frames/cards, replace modal/dropdown detours with direct inline controls where high-frequency actions live.
4. **Polish (`polish`):** Execute the final engineering pass using [polish-and-clarify.md](./references/polish-and-clarify.md): consistent control heights, tabular numbers, clear active/hover/disabled/modified states, optical alignment, keyboard shortcuts, and concise Russian microcopy.
5. **Verify (`break` & build):** Check extreme states (long filenames, empty archives, zero search matches, narrow split panels, dirty edits across tabs), then run compile & tests.

## Non-Negotiable Rules

- Never nest bordered containers inside bordered containers unless separating distinct scrollable regions.
- Never hide core high-frequency domain actions (e.g., reset camera, rename, delete, save, channel view) deep inside nested menus when toolbar space is available.
- Keep one primary accent color for active/primary calls-to-action; reserve semantic colors strictly for status (modified, added, removed, error).
- Every interactive control must show clear state feedback (hover, active/selected, disabled, dirty).
