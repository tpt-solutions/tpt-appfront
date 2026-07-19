//! Minimal SSR/SSG example: no WASM, no client-side hydration — just
//! `tpt-appfront-server` serving a static `UITree` as semantic HTML to any
//! client (crawlers, browsers, AI agents each get their preferred format).
use tpt_appfront_core::{ContainerBuilder, UITree};
use tpt_appfront_server::SmartRouterBuilder;

#[derive(Debug, Clone, serde::Serialize)]
enum Msg {}

fn build_ui() -> UITree<Msg> {
    UITree::container(|c: &mut ContainerBuilder<Msg>| {
        c.heading(1, "Welcome to AppFront").class("title");
        c.text("This page is rendered once on the server as semantic HTML.");
        c.list(|l| {
            l.text("Write your UI once as a UITree");
            l.text("Render it to DOM, Canvas, HTML, or AI-Schema");
            l.text("Serve the right backend per client automatically");
        });
    })
}

#[tokio::main]
async fn main() {
    let router = SmartRouterBuilder::new(build_ui())
        .title("AppFront SSR Example")
        .description("A minimal server-rendered AppFront page")
        .build();

    let addr = "127.0.0.1:3000".parse().unwrap();
    println!("Serving on http://{addr}");
    tpt_appfront_server::serve(router, addr).await;
}
