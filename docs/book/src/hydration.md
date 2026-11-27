# Hydration and islands

The HTML backend renders static markup. The DOM backend can take over that markup in the browser
without rebuilding it.

- Only nodes that need behaviour get listeners attached: nodes with a click or input handler, and
  nodes marked dynamic.
- Inert static content is left as it is. This is the islands model: a page is mostly static HTML, and
  only the interactive parts are live.
- The server writes a hydration payload. In the browser, `Signal::hydrated("name", default)` reads its
  value back from that payload, so state matches what the server rendered.

Turn it on with `SmartRouterBuilder::enable_hydration(true)`, and mount the DOM build on the same page.
See the [smart router](smart-router.md) chapter.
