# AI Schema Format Reference

Two machine-readable output formats that backends `appfront-ai-schema` and
`appfront-html` produce from the abstract `UITree`. The formats are designed
for crawlers (Googlebot, social-media bots) and AI agents respectively.

---

## 1. JSON-LD (Structured Data / Rich Snippets)

Standard schema.org JSON-LD embedded in `<script type="application/ld+json">`.
Target: Google rich results, knowledge panels.

### Root object

```json
{
  "@context": "https://schema.org",
  "@graph": [ … ]
}
```

### Node mapping

| `NodeKind` | `@type` | Notes |
|-----------|---------|-------|
| Container | `WebPageElement` | `hasPart` children |
| Heading   | `WebPageElement` | `headline` property |
| Text      | `WebPageElement` | `text` property |
| Button    | `Action` | `name` = label, `target` = action name |
| Input     | `PropertyValue` + `Action` | if ai.action set |
| List      | `ItemList` | `itemListElement` |
| DataGrid  | `Table` | `columnList`, `rows` |

### Example

```json
{
  "@context": "https://schema.org",
  "@graph": [
    {
      "@type": "WebPageElement",
      "name": "Dashboard",
      "headline": "Dashboard"
    },
    {
      "@type": "Table",
      "name": "Data Grid",
      "about": "Name, Value"
    },
    {
      "@type": "Action",
      "name": "Export",
      "target": {
        "@type": "EntryPoint",
        "action": "export_data"
      }
    }
  ]
}
```

---

## 2. Custom AI Schema

Optimised for AI agent consumption. Lists every interactive element, its
available action, and the parameters it expects. Also exposes current state
(input values, grid data) so an agent can understand the page without
rendering it.

### Shape

```json
{
  "schema_version": "0.1.0",
  "title": "Page or root node description",
  "interactive": [
    {
      "type": "button",
      "label": "Export",
      "action": "export_data",
      "params": { }
    },
    {
      "type": "input",
      "value": "...",
      "action": "set_value",
      "params": { "value": "..." }
    }
  ],
  "data": [
    {
      "type": "data_grid",
      "columns": ["Name", "Value"],
      "rows": [ ["a", "1"], ["b", "2"] ]
    }
  ]
}
```

### Fields

- **`interactive`** — array of elements an agent can programmatically
  interact with (buttons, inputs, and any node that carries an `on_click` or
  `AiMeta.action`).
  - `type` — `"button"` | `"input"`
  - `label` / `value` — visible text or current value
  - `action` — machine-readable action identifier from `AiMeta.action`
  - `params` — key-value parameter map from `AiMeta.params`
- **`data`** — read-only data views (data grids, lists, text) that describe
  the page content so the agent can answer questions about it.

---

## 3. HTML data attributes

The `appfront-html` backend embeds the same metadata in the DOM so any
HTML-aware crawler or agent can recover it by scraping:

```html
<button data-ai-action="export_data" data-ai-params='{}'>Export</button>
<input  data-ai-action="set_value" data-ai-params='{"value":""}' />
```

OpenGraph meta tags are emitted at the page level by `render_page()`:

```html
<meta property="og:title" content="…" />
<meta property="og:description" content="…" />
<meta property="og:type" content="website" />
```
