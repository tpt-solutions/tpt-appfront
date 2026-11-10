//! Interactive demo of the internal window layer: a counter button, a text
//! field, and a checkbox, all driven through `run_window`.
//!
//! Run: `cargo run -p tpt-appfront-canvas --example window_demo`

use std::cell::RefCell;
use std::rc::Rc;

use tpt_appfront_canvas::run_window;
use tpt_appfront_core::UITree;

#[derive(Clone, Debug)]
enum Msg {
    Count,
    Name(String),
    Agree(bool),
}

#[derive(Default)]
struct State {
    count: u32,
    name: String,
    agree: bool,
}

fn main() {
    let state = Rc::new(RefCell::new(State::default()));
    let for_view = state.clone();
    let for_update = state.clone();

    let result = run_window(
        "window demo",
        480,
        300,
        move || {
            let s = for_view.borrow();
            UITree::container(|c| {
                c.heading(2, "Internal window layer");
                c.text(format!("Clicked {} times", s.count));
                c.button("Count up").on_click(Msg::Count);
                c.input(s.name.clone()).on_input(Msg::Name);
                c.checkbox("I agree", s.agree).on_toggle(Msg::Agree);
            })
        },
        move |msg| {
            let mut s = for_update.borrow_mut();
            match msg {
                Msg::Count => s.count += 1,
                Msg::Name(name) => s.name = name,
                Msg::Agree(agree) => s.agree = agree,
            }
        },
    );

    if let Err(e) = result {
        eprintln!("{e}");
        std::process::exit(1);
    }
}
