//! A viewport is a node the host fills with pixels from a frame source named by a binding.

use tpt_appfront_core::ui_tree::{NodeKind, UITree};

#[test]
fn a_viewport_is_a_node_with_its_binding_and_label() {
    let ui: UITree<()> = UITree::container(|c| {
        c.viewport("scene", "A lit cube");
    });
    let NodeKind::Container { children } = &ui.kind else {
        panic!("the root is a container");
    };
    match &children[0].kind {
        NodeKind::Viewport { name, alt } => {
            assert_eq!(name, "scene");
            assert_eq!(alt, "A lit cube");
        }
        _ => panic!("the child is a viewport"),
    }
}
