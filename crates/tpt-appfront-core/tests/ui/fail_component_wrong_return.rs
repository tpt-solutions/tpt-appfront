use tpt_appfront_core::component;

#[component]
fn not_a_view() -> i32 {
    // `#[component]` only accepts functions that return `UITree<Msg>`.
    1
}

fn main() {
    let _ = not_a_view();
}
