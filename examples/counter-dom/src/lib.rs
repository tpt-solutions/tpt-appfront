use tpt_appfront_core::{create_effect, Signal, UITree};
use std::rc::Rc;
use wasm_bindgen::prelude::*;

#[derive(Debug, Clone)]
enum Msg {
    Increment,
}

#[wasm_bindgen(start)]
pub fn start() -> Result<(), JsValue> {
    console_error_panic_hook::set_once();

    let window = web_sys::window().expect("no window");
    let document = window.document().expect("no document");
    let body = document.body().expect("no body");

    let container = document.create_element("div")?;
    body.append_child(&container)?;

    let count = Signal::new(0i32);
    let display = Signal::new(format!("Count: {}", count.get()));

    let count_for_effect = count.clone();
    let display_for_effect = display.clone();
    let handle = create_effect(move || {
        display_for_effect.set(format!("Count: {}", count_for_effect.get()));
    });
    std::mem::forget(handle);

    let ui: UITree<Msg> = UITree::container(|c| {
        c.heading(1, "Counter");
        c.button("+1").on_click(Msg::Increment);
    });

    let count_for_dispatch = count.clone();
    let dispatch: Rc<dyn Fn(Msg)> = Rc::new(move |msg| match msg {
        Msg::Increment => count_for_dispatch.set(count_for_dispatch.get() + 1),
    });

    tpt_appfront_dom::mount(&container, &ui, dispatch)?;

    let (text_node, text_handle) = tpt_appfront_dom::reactive_text(&document, display)?;
    container.append_child(&text_node)?;
    // Whole-process root mount: forgetting is an explicit choice here, not
    // reactive_text's default behavior.
    std::mem::forget(text_handle);

    Ok(())
}
