//! The canvas backend keeps the latest frame per viewport binding and refuses bad buffers.

use tpt_appfront_canvas::Scene;

#[test]
fn a_frame_is_stored_for_its_binding_when_the_buffer_matches_the_size() {
    let mut scene = Scene::new();
    assert!(scene.viewport_frame("scene").is_none());
    assert!(scene.set_viewport_frame("scene", 2, 2, vec![0; 16]));
    let frame = scene.viewport_frame("scene").expect("the frame is stored");
    assert_eq!((frame.width, frame.height), (2, 2));
    assert_eq!(frame.rgba.len(), 16);
}

#[test]
fn a_wrong_sized_or_empty_frame_is_refused() {
    let mut scene = Scene::new();
    assert!(!scene.set_viewport_frame("scene", 2, 2, vec![0; 15]));
    assert!(!scene.set_viewport_frame("scene", 0, 2, Vec::new()));
    assert!(scene.viewport_frame("scene").is_none());
}
