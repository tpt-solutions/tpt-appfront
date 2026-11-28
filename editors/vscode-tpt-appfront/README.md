# TPT AppFront: view! syntax for VS Code

Highlights the markup inside `view! { ... }` in Rust files: tag names, attribute names, string children, and `{ ... }` expressions (which are highlighted as Rust).

It is an injection grammar, so it adds to Rust highlighting and does not replace it. The `view!` snippets live in the repo's `.vscode/tpt-appfront.code-snippets`.

## Use it

Copy this folder into your VS Code extensions directory and reload the window, or package it with `vsce package` and install the `.vsix`. It is not published to the marketplace.

## Test

The grammar is checked with `vscode-textmate`, the tokenizer VS Code uses, over a sample block. Real VS Code was not launched for this check.
