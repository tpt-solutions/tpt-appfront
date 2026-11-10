use tpt_appfront_core::{view, UITree};

#[derive(Debug, Clone, PartialEq)]
enum Msg {
    Increment,
}

fn broken() -> UITree<Msg> {
    // A `Button` without its required `on_click` attribute.
    view! {
        <Container>
            <Button>"+1"</Button>
        </Container>
    }
}

fn main() {
    let _ = broken();
}
