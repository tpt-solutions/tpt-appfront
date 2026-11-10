use tpt_appfront_core::{view, UITree};

#[derive(Debug, Clone, PartialEq)]
enum Msg {
    Increment,
}

fn counter() -> UITree<Msg> {
    view! {
        <Container>
            <Heading level={1u8}>"Counter"</Heading>
            <Button on_click={Msg::Increment}>"+1"</Button>
        </Container>
    }
}

fn main() {
    let _ = counter();
}
