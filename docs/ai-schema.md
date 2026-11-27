# AI Schema Format Reference

The formats that `tpt-appfront-ai-schema` and `tpt-appfront-html` produce from the abstract
`UITree`. They are for two audiences:

- **Crawlers and social-media bots** read JSON-LD (section 1) and OpenGraph tags (section 4).
- **AI agents** read the custom AI schema (section 2), the `llms.txt` summary (section 3), and
  the OpenAPI description of the page's actions (section 3). They act on the page through
  `POST /command` (section 5) or the MCP server.

The formats are frozen: adding a field is compatible, renaming or removing one is not.
`schema_version` in the AI schema reports the version (`0.1.0`).

---

## 1. JSON-LD (structured data for rich results)

Schema.org JSON-LD, emitted in `<script type="application/ld+json">`. Produced by
`tpt_appfront_ai_schema::to_json_ld`.

### Root object

```json
{
  "@context": "https://schema.org",
  "@graph": [ … ]
}
```

### Node mapping

| `NodeKind` | `@type` | Properties |
|---|---|---|
| `Container` | `WebPageElement` | Children are walked into the graph; `hasPart` is added when there are children |
| `Heading` | `WebPageElement` | `headline` |
| `Text` | `WebPageElement` | `text` |
| `Button` | `Action` (with `EntryPoint`) when `AiMeta.action` is set; otherwise an action entry when `on_click` is set | `name` = label, `target.action` = action name |
| `Input` | `WebPageElement` | `name: "input"`, `value` |
| `Textarea` | `WebPageElement` | `name: "textarea"`, `value` |
| `Checkbox` | `WebPageElement` | `name` = label, `value` (boolean) |
| `Select` | `WebPageElement` | `name: "select"`, `value` = selected option |
| `Radio` | `WebPageElement` | `name` = group name, `value` = selected option |
| `List` | `ItemList` | `itemListElement` (a `ListItem` per child) |
| `DataGrid` | `Table` | `about` = column names |
| `Image` | `ImageObject` | `contentUrl` = src, `description` = alt |
| `Link` | `WebPageElement` | `url` = href, `name` = text |
| `Media` | `AudioObject` or `VideoObject` | `contentUrl` = src, `description` = alt |
| `Viewport` | `WebPageElement` | `name` = `alt` |
| `Portal` | (content walked) | The portal's content is emitted in place |

### Example

```json
{
  "@context": "https://schema.org",
  "@graph": [
    { "@type": "WebPageElement", "headline": "Dashboard" },
    { "@type": "Table", "about": "Name, Value" },
    {
      "@type": "Action",
      "name": "Export",
      "target": { "@type": "EntryPoint", "action": "export_data" }
    }
  ]
}
```

---

## 2. Custom AI schema

Produced by `tpt_appfront_ai_schema::to_ai_schema` (or `to_ai_schema_value` for a
`serde_json::Value`). It lists every interactive element with its action and params, and the
readable content with its current values, so an agent can understand the page without rendering
it.

### Shape

```json
{
  "schema_version": "0.1.0",
  "title": "Orders",
  "interactive": [
    { "type": "button", "label": "Export", "value": null,
      "action": "export_data", "params": {} },
    { "type": "input", "label": null, "value": "",
      "action": "set_filter", "params": {} }
  ],
  "data": [
    { "type": "heading", "text": "Orders" },
    { "type": "data_grid", "columns": ["Name", "Value"], "rows": [["a", "1"], ["b", "2"]] }
  ]
}
```

### Fields

- **`title`**: the root node's `AiMeta.title` when set. Otherwise the first `data` element's text.
  Set `AiMeta.title` on the root so a breadcrumb or aside cannot mis-name the page.
- **`interactive`**: one entry per control an agent can operate.
  - `type`: `button`, `input`, `textarea`, `checkbox`, `select` or `radio`.
  - `label`: visible text. Buttons and checkboxes have one. Fields do not.
  - `value`: the current value. Checkboxes report `"true"` or `"false"`.
  - `action`: the machine-readable name from `AiMeta.action`. Omitted when unset.
  - `params`: the key-value map from `AiMeta.params`.
- **`data`**: read-only content, in document order. `type` is one of:
  - `heading` and `text` (`text`);
  - `data_grid` (`columns`, `rows`);
  - `image`, `link`, `audio`, `video` and `viewport` (`text` = the alt text or link text).

  Lists and portals are walked, so their children appear in `interactive` or `data` as well.

---

## 3. `llms.txt` and OpenAPI

Two exports built from the same AI schema, for agents that discover pages by convention.

- **`to_llms_txt(&ui) -> String`** writes Markdown: a `#` title, a `## Content` list of the `data`
  entries, and a `## Interactive elements` list such as
  ``- button "Export", action `export_data`, params format``. Serve it at `/llms.txt`.
- **`to_openapi(&ui) -> serde_json::Value`** is an OpenAPI 3.1 document describing `POST /command`.
  Its `action` property is an `enum` of the actions this page exposes, so a generated client can
  only call what the page offers. `params` is a free-form object.

---

## 4. HTML data attributes and OpenGraph

`tpt-appfront-html` embeds the same metadata in the DOM, so a scraper can recover it without
running the app:

```html
<button data-ai-action="export_data" data-ai-params='{}'>Export</button>
<input  data-ai-action="set_value" data-ai-params='{"value":""}' />
```

`render_page()` adds OpenGraph tags for social-bot crawls:

```html
<meta property="og:title" content="…" />
<meta property="og:description" content="…" />
<meta property="og:type" content="website" />
```

---

## 5. Calling an action: `POST /command` and MCP

Agents run an action with a JSON request:

```http
POST /command
Content-Type: application/json

{ "action": "export_data", "params": { "format": "csv" } }
```

`tpt-appfront-server` serves this route. The app supplies the handler through
`SmartRouterBuilder::on_command`. Its rules:

- The body is size-limited.
- `action` must not be empty. When `allowed_actions` is set, it must be on that list.
- The route is rate-limited, and it returns `501` when no handler is configured.

`tpt-appfront-mcp` exposes the same actions as MCP tools, one per interactive node that has an
`AiMeta.action`, plus `query_state` and `navigate`. The MCP server speaks JSON-RPC over stdio.

Use `to_openapi` to describe the `/command` route for a page, and the MCP server to expose the
same page to a local agent.
