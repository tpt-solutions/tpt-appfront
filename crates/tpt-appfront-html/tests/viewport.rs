//! A viewport renders as a canvas the host draws into, found by its binding name.

use tpt_appfront_core::ui_tree::UITree;
use tpt_appfront_html::render;

#[test]
fn a_viewport_renders_as_a_named_canvas() {
    let ui: UITree<()> = UITree::container(|c| {
        c.viewport("scene", "A lit cube");
    });
    let html = render(&ui);
    assert!(html.contains("<canvas"), "{html}");
    assert!(html.contains("data-viewport=\"scene\""), "{html}");
    assert!(html.contains("aria-label=\"A lit cube\""), "{html}");
    assert!(html.contains("</canvas>"), "{html}");
}
