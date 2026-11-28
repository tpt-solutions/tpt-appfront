//! A locale change shows on the next frame. The TUI rebuilds its `UITree` on each
//! frame, so an app that calls `t!` while building the tree needs no backend
//! support: the new translation appears on the next render.

use tpt_appfront_core::i18n::{self, Catalog};
use tpt_appfront_core::{t, ContainerBuilder, UITree};
use tpt_appfront_tui::{buffer_to_string, render_to_buffer};

#[derive(Debug, Clone)]
enum Msg {}

fn screen() -> UITree<Msg> {
    UITree::container(|c: &mut ContainerBuilder<Msg>| {
        c.text(t!("status.ready"));
    })
}

#[test]
fn locale_change_appears_on_the_next_frame() {
    let mut catalog = Catalog::new("en");
    catalog.add("en", &[("status.ready", "Ready")]);
    catalog.add("de", &[("status.ready", "Bereit")]);
    i18n::install(catalog);

    i18n::set_locale("en");
    let before = buffer_to_string(&render_to_buffer(&screen(), 20, 3));
    assert!(before.contains("Ready"), "{before}");

    i18n::set_locale("de");
    let after = buffer_to_string(&render_to_buffer(&screen(), 20, 3));
    assert!(after.contains("Bereit"), "{after}");
    assert!(!after.contains("Ready"), "{after}");
}
