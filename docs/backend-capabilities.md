# Backend capability matrix

Which `NodeKind` each backend renders, and how completely. Checked against the source in this
repository; when a backend changes, update the matches here.

Key:

- **Full**: renders the node and handles its interaction (pointer, keyboard, two-way binding).
- **Partial**: renders it, but part of the behaviour is missing. The note says what.
- **Display**: renders the node's content only; no interaction.
- **Not rendered**: the backend ignores the node.

Backends:

- **DOM**: `tpt-appfront-dom` (wasm32, real DOM).
- **Canvas**: `tpt-appfront-canvas` (software raster in an OS window, or `<canvas>` on web).
- **TUI**: `tpt-appfront-tui` (ratatui terminal).
- **HTML**: `tpt-appfront-html` (SSR/SSG string). Output only, so it has no interaction column.

| Node kind | DOM | Canvas | TUI | HTML |
|---|---|---|---|---|
| `Container` | Full | Full (flex layout) | Full (layout split) | Full |
| `List` | Full (keyed diff, window with virtual scroll) | Full | Full (list widget) | Full |
| `Heading` | Full | Full | Full | Full |
| `Text` | Full | Full | Full | Full |
| `Button` | Full (click, keyboard) | Full (click, Enter, Space) | Full (Enter, Space) | Full |
| `Input` | Full | Full (controlled, `on_input`) | Full (`on_input`, Space types a space) | Full |
| `Textarea` | Full | Full (Enter adds a line) | Partial: Enter adds a line and dispatches `on_input`; no separate line editing | Full |
| `Checkbox` | Full | Full (click, Space, `on_toggle`) | Full (Enter, Space, `on_toggle`) | Full |
| `Select` | Full | Partial: painted as the current option; click, Enter and arrows step through options and dispatch `on_input`. No dropdown list | Full (arrows and Enter step through options, `on_input`) | Full |
| `Radio` | Full | Partial: painted as one row; focus, click, Enter and arrows step through options and dispatch `on_input`. No per-option hit target, so a click on one option does not pick that option | Full (arrows and Enter, `on_input`) | Full |
| `DataGrid` | Full (keyed rows, window with virtual scroll) | Full | Full (table widget) | Full |
| `Portal` | Partial: rendered inline; no overlay target layer | Partial: rendered inline; no overlay target layer | Partial: rendered inline; no overlay target layer | Partial: rendered inline |
| `Image` | Full (`<img>`) | Full (PNG, JPEG, GIF, WebP first frame; host-supplied bytes; see `image.rs`) | Display (alt caption) | Full |
| `Link` | Full (`<a>`) | Partial: painted; no click navigation | Display | Full |
| `Media` | Full (`<audio>`/`<video>`) | Display (labelled fallback) | Display (labelled fallback) | Full |
| `Viewport` | Full (`<canvas>` bound by name) | Full (host frames by name) | Display (`alt` text) | Full |

## Event bindings

| Binding | DOM | Canvas | TUI |
|---|---|---|---|
| `on_click` | Yes | Yes | Yes (Enter, Space) |
| `on_input` | Yes | Yes | Yes (typing, Backspace, Enter in textarea, select/radio steps) |
| `on_toggle` | Yes | Yes | Yes (Enter, Space) |
| `on_focus` / `on_blur` | Yes | Yes | Yes (Tab, arrows) |
| `on_keydown` | Yes | Yes | Yes (receives DOM-style key names) |
| Pointer down/move/up | Yes | Yes | Not applicable (keyboard only) |

## Routing

There is no separate routing mount helper for canvas and TUI. Both backends rebuild the UI from a
closure on every frame, so a route is `move || router.current_view()`. The DOM is the only backend
that needs `mount_router`, because it also syncs the browser URL and reconciles DOM nodes. The
TUI re-collects its focusable nodes each frame (`TuiDriver::refresh`), so a route change keeps
keyboard navigation correct.

## Known limitations

- **TUI interaction state is keyed by position.** `inputs`, `checks` and `selections` follow
  the node's position in the tree, not its `key`. A route change that reorders controls can move
  one control's typed value onto another. Keying these maps by `NodeMeta::key` would fix it.
- **Portals are not overlays on any backend.** Use `UITree::collect_portals` if the app needs a
  separate layer.
- **Canvas `Radio` has no per-option pointer target.**
- **Canvas `Link` does not navigate on click.**

## Not a rendering backend

`tpt-appfront-ai-schema` (JSON-LD and the AI schema) and `tpt-appfront-mcp` describe the tree
rather than drawing it. See [ai-schema.md](ai-schema.md).
