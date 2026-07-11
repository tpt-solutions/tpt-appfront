//! Compiles every `view!` snippet emitted by `appfront-cli`'s
//! `generate --prompt` scaffolder (see `crates/appfront-cli/src/generate.rs`)
//! against a real `Msg` enum, proving the generator's output is not just
//! plausible-looking but actually valid `view!` syntax. Kept in appfront-core
//! (rather than appfront-cli) since it needs `appfront_core::view!` in scope
//! the same way a scaffolded app would.

use appfront_core::{view, UITree};

#[derive(Clone, Debug, PartialEq)]
enum Msg {
    Increment,
    Submit,
    AddTask,
    CardAction,
    NavHome,
    NavAbout,
}

#[test]
fn counter_snippet_compiles() {
    let _ui: UITree<Msg> = view! {
        <Container class="counter">
            <Heading level={1u8}>"Count: 0"</Heading>
            <Button on_click={Msg::Increment}>"+1"</Button>
        </Container>
    };
}

#[test]
fn login_form_snippet_compiles() {
    let _ui: UITree<Msg> = view! {
        <Container class="login-form">
            <Heading level={1u8}>"Sign in"</Heading>
            <Input value="Email" />
            <Input value="Password" />
            <Button on_click={Msg::Submit}>"Sign in"</Button>
        </Container>
    };
}

#[test]
fn todo_list_snippet_compiles() {
    let _ui: UITree<Msg> = view! {
        <Container class="todo-list">
            <Heading level={1u8}>"Tasks"</Heading>
            <Input value="New task" />
            <Button on_click={Msg::AddTask}>"Add"</Button>
        </Container>
    };
}

#[test]
fn card_snippet_compiles() {
    let _ui: UITree<Msg> = view! {
        <Container class="card">
            <Heading level={2u8}>"Title"</Heading>
            <Text>"Description goes here."</Text>
            <Button on_click={Msg::CardAction}>"Learn more"</Button>
        </Container>
    };
}

#[test]
fn nav_bar_snippet_compiles() {
    let _ui: UITree<Msg> = view! {
        <Container class="navbar">
            <Heading level={1u8}>"Brand"</Heading>
            <Button on_click={Msg::NavHome}>"Home"</Button>
            <Button on_click={Msg::NavAbout}>"About"</Button>
        </Container>
    };
}

#[test]
fn fallback_snippet_compiles() {
    let _ui: UITree<Msg> = view! {
        <Container class="app">
            <Heading level={1u8}>"Hello"</Heading>
            <Text>"Describe your UI more specifically (e.g. \"counter\", \"login form\", \"todo list\", \"card\", \"nav bar\") for a closer starting point."</Text>
        </Container>
    };
}
