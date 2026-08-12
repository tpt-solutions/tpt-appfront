# Changelog

All notable changes to `tpt-appfront-html` will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- `data-ai-action` / `data-ai-params` attributes emitted on interactive nodes for
  AI crawler discovery.
- `render_page` emits OpenGraph meta tags for social bots.
- Utility-class rendering: `styling::inline_style` is applied as a `style="..."`
  attribute alongside `class`.
- Accessibility: form controls (`Input`/`Textarea`/`Select`/`Checkbox`/`Radio`) emit a
  stable `id` (`af-<data_appfront_id>`); `Checkbox`/`Radio` use an explicit `<label for>`
  association; `Select` surfaces `meta.ai.description` as `aria-label`; `DataGrid` renders
  `role="grid"` plus `role="row"`/`role="columnheader"`/`role="gridcell"`.

## [0.1.0]

### Added
- Initial release: `render` (semantic HTML fragment) and `render_page` (full HTML5
  page) from a `UITree<Msg>`; renders the full list (ignores `VirtualScroll`) so
  crawlers receive complete content.
