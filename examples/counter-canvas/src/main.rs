use appfront_core::{Signal, UITree};

#[derive(Debug, Clone)]
enum Msg {
    Increment,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let count = Signal::new(0i32);

    let count_for_ui = count.clone();
    let build_ui = move || -> UITree<Msg> {
        UITree::container(|c| {
            c.heading(1, "Counter");
            c.text(format!("Count: {}", count_for_ui.get()));
            c.button("+1").on_click(Msg::Increment);
        })
    };

    let dispatch = move |msg: Msg| match msg {
        Msg::Increment => count.set(count.get() + 1),
    };

    appfront_canvas::run_native("Counter", build_ui, dispatch)?;
    Ok(())
}
