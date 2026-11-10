use tpt_appfront_core::{view, UITree};

#[derive(Debug, Clone, PartialEq)]
enum Msg {
    Increment,
}

fn broken() -> UITree<Msg> {
    // A misspelled `on_click`: the error should suggest the right attribute.
    view! {
        <Container>
            <Button on_clcik={Msg::Increment}>"+1"</Button>
        </Container>
    }
}

fn main() {
    let _ = broken();
}
