use tpt_appfront_core::{view, UITree};

#[derive(Debug, Clone, PartialEq)]
enum Msg {
    Increment,
}

fn broken() -> UITree<Msg> {
    // A typo of the built-in `Container` tag: the error should name the tag.
    view! {
        <Contaner>
            <Text>"hello"</Text>
        </Contaner>
    }
}

fn main() {
    let _ = broken();
}
