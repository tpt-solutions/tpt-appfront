# Migrating to TPT AppFront

These guides map the concepts you already use onto AppFront. Each one has the same shape:

1. **Concept map**: a table from the old framework's names to AppFront's.
2. **Side-by-side**: one small component, before and after.
3. **Gotchas**: where the models differ and the port will need thought.

- [From React](react.md)
- [From Leptos](leptos.md)
- [From Dioxus](dioxus.md)

**Read this first.** AppFront is not a virtual-DOM framework and does not have hooks, a
component lifecycle, or a compiler-driven reactive graph like Leptos. Its core is a plain
`UITree<Msg>` value plus a small signal system. The DOM backend mounts that tree once and updates
only the nodes whose signals changed. Most ports are a rewrite of the state layer, not a
line-by-line translation of the JSX or RSX.

The framework-side code in these guides is written from the public APIs of React, Leptos and
Dioxus, not compiled here. The AppFront side mirrors the tested recipes in
[`docs/cookbook.md`](../cookbook.md). If a framework snippet looks off, check it against that
framework's current documentation.
