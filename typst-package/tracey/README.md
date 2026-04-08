# tracey

Requirement markers for [tracey](https://github.com/tracey-rs/tracey)-traced
specifications.

```typst
#import "@preview/tracey:0.1.0": r

= My Spec

#r("auth.login")[Users must provide valid credentials to log in.]
```

Tracey itself does **not** require this import — it injects its own
definitions of `#r` / `#req` when rendering specs for the dashboard. Importing
the package is recommended anyway: it lets the spec compile standalone with
`typst compile` (PDF preview, CI checks) and gives the Typst language server a
definition to type-check against.
