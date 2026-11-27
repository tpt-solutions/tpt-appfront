# Styling with class!

Styling uses a set of curated utility classes. The `class!` macro checks each name when the program
compiles, so a misspelt utility is a build error.

```rust,ignore
use tpt_appfront_core::class;

let card = class!("flex-col", "gap-4", "p-4", "rounded-lg");
let title = class!("font-bold", "text-center");
```

- Each literal must be a known utility. An unknown name fails the build.
- The result is a `String` you pass to a node, for example `UITree::container(..).class(card)`.
- Backends apply the classes they support. The canvas paints a subset, and a test lists the utilities
  it does not render, so the gap is explicit. See `docs/backend-capabilities.md` in the repository.
