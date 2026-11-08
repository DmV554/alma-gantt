# 0009 — Render pipeline: display list, deterministic SVG, PNG through resvg

- **Status:** accepted
- **Date:** 2026-10-29

## Context
M0 needs `alma export --svg/--png` so an agent can look at its own plan (SPEC.md §5, §8a).
SPEC.md §9 asks for one layout engine shared by screen and print, and §13 warns that text
metrics must match between the browser and the exporter, which requires a bundled font.

## Decision
- `alma-layout` turns a scheduled plan into a display list (rectangles, lines, polylines, text,
  clips) in points. It is pure: no I/O, no clock.
- `alma-export` writes the display list as SVG with a fixed number format (at most two
  decimals), so the same plan gives the same bytes. PNG rasterizes that SVG with `resvg` 0.49
  (MIT/Apache; its dependency tree is MIT, Apache, BSD, Zlib and Unicode licensed).
- Until a font is bundled, text widths are estimated from an average glyph width (0.54 em,
  0.58 em bold) and PNG uses the system's sans-serif fonts. The SVG names the font stack
  `IBM Plex Sans, Noto Sans, DejaVu Sans, Arial, sans-serif`.
- No text is ever smaller than 9 pt (SPEC.md §1).

## Consequences
SVG output is stable and testable; PNG pixels may differ slightly between machines and labels
can be a little shorter or longer than estimated. Bundling a font (license review needed:
OFL-1.1 fonts are the likely choice) is required before print preflight (M3) and the desktop
UI, and will replace the estimate with real metrics in both.

Addendum (2026-11-08, M2): the resource histogram and the S-curve use the same pipeline. Pure
layout functions in `alma_layout::charts` take per-period numbers already computed by the engine
and return a display list. `alma export --chart histogram|scurve` and MCP `render` turn it into
SVG or PNG.
