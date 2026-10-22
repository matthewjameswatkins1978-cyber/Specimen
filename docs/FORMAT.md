# Specimen project format v1

The conventional suffix is `.specimen.json`. UTF-8 JSON is the canonical
editable document. Fonts are family names, not embedded files. Pixel output
therefore depends on the available fonts; the font list and inspection report
make that dependency observable. Parley supplies platform fallback.

```json
{
  "version": 1,
  "title": "New signal",
  "width": 900,
  "height": 1200,
  "background": "#181D20",
  "elements": [
    {
      "id": "title",
      "name": "Headline",
      "x": 48.0, "y": 100.0, "w": 804.0, "h": 160.0,
      "color": "#EEE9D9",
      "kind": {
        "type": "text",
        "text": "NEW SIGNAL",
        "font": "Arial", "weight": 700, "size": 100.0,
        "tracking": 0.0, "leading": 1.0, "align": "left"
      }
    }
  ]
}
```

Coordinates and font sizes are document pixels, independent of window size.
Tracking is extra inter-glyph spacing in pixels. Leading is a font-size-relative
line-height multiplier. Alignment is `left`, `center` or `right` inside the
element rectangle. Elements paint from first to last. The same order determines
canvas selection (frontmost bounding box). Shape kinds are
`{"type":"rectangle"}` and `{"type":"ellipse"}`.

Text clips to its element rectangle. Artwork clips to the document boundary.
Inspection exposes unclipped glyph ink bounds and overflow rather than silently
claiming all text fits. There is no rotation, alpha colour, font embedding or
raster-image layer in v1.

Validation rejects unknown fields, unsupported versions, duplicate/empty IDs,
non-finite geometry, invalid colours and out-of-budget documents. Canvas:
64..8192 px per side, at most 32 million pixels; at most 128 elements; file
input at most 4 MB. Text: at most 20,000 UTF-8 bytes per layer; font size
1..1000, weight 1..1000, tracking -50..200, leading 0.5..5.
History keeps 100 document snapshots and is session-local.

Save writes and syncs a uniquely named sibling temporary file, then renames it
over the destination. Closing with unsaved committed document changes creates
a uniquely named recovery project in the evidence directory. Uncommitted field
drafts are not part of recovery. Previous recovery files are retained.

PNG export uses the exact full-resolution artwork buffer shown by the canvas,
scaled only for screen presentation. Guides, handles and application chrome
are excluded. A native `name.specimen.json` exports to `name.png`; any other
project filename exports to `filename.png` so export cannot target the active
project itself.
