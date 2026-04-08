// Standalone definitions for tracey requirement markers.
//
// Importing this package lets a spec compile with `typst compile` directly
// (PDF, etc.) and gives editors a definition for `#r` / `#req`. When tracey
// itself renders the spec for the dashboard it strips this import and injects
// its own HTML-emitting definitions instead.

#let _badge(body, fill: luma(235)) = box(
  fill: fill,
  inset: (x: 0.5em, y: 0.25em),
  radius: 3pt,
  // `raw` picks typst's bundled monospace face, avoiding hard-coded font
  // names that warn when unavailable on the host.
  text(size: 0.8em, raw(body)),
)

#let req(id, level: none, status: none, ..meta, body) = block(
  width: 100%,
  stroke: (left: 2pt + luma(180)),
  inset: (left: 1em, rest: 0.6em),
  spacing: 1.2em,
  {
    _badge(id)
    if level != none { h(0.4em); _badge(fill: rgb("#e8f0fe"), level) }
    if status != none { h(0.4em); _badge(fill: rgb("#fef7e0"), status) }
    linebreak()
    v(0.4em)
    body
  },
)

#let r = req
