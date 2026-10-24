# Specimen 032 — visual evidence and self-critique

Evidence for PR #3 review findings. All shots are real application
frames captured through the Eye snapshot path from live windows;
every claim below was checked against layout telemetry, not
eyeballed from pixels.

## Matrix

| Shot | Window | State |
|---|---|---|
| 1440-fit-expanded.png | 1440x980 | Fresh template open, library expanded |
| 1440-library-collapsed.png | 1440x980 | Library collapsed: 8 layer rows |
| 1440-inspector-type.png | 1440x980 | Title selected, Type tab |
| 1440-zoom-selection.png | 1440x980 | Selection zoom on coordinates, 548% |
| 1440-zoom-actual.png | 1440x980 | Actual size, 100% |
| 1100-fit-expanded.png | 1100x820 | Minimum window, library expanded |
| 1100-inspector-type.png | 1100x820 | Minimum window, title selected |

## Composition judgement

The three-region structure (library/layers, canvas, inspector) holds
at both sizes. The canvas keeps prominence; the toolbar reads in
draw/history/view groups; the inspector reads as grouped tool panels
with visible state (active tab, active alignment, active weight,
followed layer highlight). Nothing clips, nothing overlaps, nothing
paints outside its panel in any shot — verified per-node in telemetry,
not just by looking.

## Defects found independently and fixed

1. **Inspector hint painted over the footer at 1100x820.**
   The full Type tab ran 18px past the inspector panel; the last
   hint row (`undo-hint`, bottom=808 vs panel bottom=790,
   clipped=True) rendered on top of the footer status bar. Unit
   tests missed it because only buttons were in the clip contract.
   Fix: drop the redundant hint row (undo/redo stay discoverable
   through the toolbar buttons; Enter/Esc has no other affordance).
   Regression cover: labels joined the no-clip contract in the
   seeded workspace test, which now passes across the full
   size/selection/tab matrix.

## Honest limitations (not fixed)

- **Zoomed text is bilinear-upscaled**, visibly soft past ~200%.
  Honest for layout inspection; not for judging type rendering.
  Crisp zoom would need raster-at-scale (perf cost). Out of scope.
- **Collapsed library leaves slack at 980p+** from the 8-row cap.
  Left as a scannability trade-off; expanding the cap is one
  constant plus re-verification if Matthew wants it.
- **Template thumbnails stay small.** Portrait art in a 212px
  sidebar cannot preview large without stealing layer rows.
  Layers won that trade deliberately; collapse gives the choice.
- **"Next available font" stays a crutch.** 235 installed families
  need a filterable list control Hamesh does not ship (registry has
  4 icons, no dropdown/list primitive). Recorded as a framework
  requirement, not app-hacked.

## Deliberate choices, verified not defects

- 36px toolbar buttons with 6px margins (measured, not squeezed).
- 5px selection handles (visual only; drag works anywhere on the
  element, so no hit-area problem exists).
- Two header/toolbar bars totalling 110px (the filler label is gone;
  everything remaining is functional chrome).
- Accepted poster artwork untouched throughout.

## Process notes

- Twice I nearly "fixed" non-defects: a header suffix that looked
  clipped in a scaled view (full-res crop + Eye advance proved it
  whole), and a row count that contradicted hand arithmetic (the
  live viewport was 1440x980, not the assumed 960). Verify the
  viewport and the pixels before mutating.
- The before-state of defect 1 survives as Eye telemetry
  (undo-hint bottom=808, clipped=True); its screenshot was
  overwritten during evidence recapture. The regression test now
  guards the class, not just the instance.
