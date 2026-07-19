//! A small, backend-agnostic component library / design system.
//!
//! Every widget here builds a plain [`UITree<Msg>`] out of the core node kinds
//! (`Container`, `List`, `Button`, `Input`, `DataGrid`, `Portal`, `Text`), so
//! the same components render on the DOM, canvas, and TUI backends without any
//! backend-specific code. State is held in [`Signal`]/[`Store`], and user
//! intent is surfaced via `Msg` (`on_click`/`on_input`), exactly like the rest
//! of the framework.
//!
//! The set covers the gaps called out in `todo.md` Phase 16: `Modal`, `Tabs`,
//! `Dropdown`, `SortableTable`, and `DatePicker`. Each is intentionally
//! minimal — a real design system would theme these via the
//! [`crate::styling`] utility classes — but they are complete and testable
//! primitives you can compose into an app today.

use crate::resource::Resource;
use crate::signal::{create_effect, EffectHandle};
use crate::ui_tree::{ContainerBuilder, UITree};

/// A single tab's body builder, stored boxed in [`tabs`].
type TabContent<Msg> = Box<dyn FnOnce(&mut ContainerBuilder<Msg>) + 'static>;

/// A modal dialog. Visible only when `open` is `true`; renders its `body`
/// inside a [`NodeKind::Portal`] (so backends show it as an overlay) plus a
/// close button wired to `on_close`. When closed it renders an empty container
/// (no DOM nodes), so it costs nothing while hidden.
pub fn modal<Msg: Clone + 'static>(
    open: bool,
    title: impl Into<String>,
    body: impl FnOnce(&mut ContainerBuilder<Msg>),
    on_close: Msg,
    target: impl Into<String>,
) -> UITree<Msg> {
    let title = title.into();
    if !open {
        return UITree::container(|_| {});
    }
    UITree::container(|c| {
        c.portal(target, |p| {
            let dlg = p.container(|d| {
                d.heading(2, title.clone());
                body(d);
                d.button("Close")
                    .on_click(on_close)
                    .attr("aria-label", "Close dialog");
            });
            dlg.attr("role", "dialog")
                .attr("aria-modal", "true")
                .aria("label", title.clone());
        });
    })
}

/// A tabbed panel. `active` is the index of the selected tab; `on_select`
/// receives the newly-selected index when a tab header is clicked. `tabs` are
/// `(label, content)` pairs. The active tab's body is shown; the others are
/// hidden. Each tab header is a `role="tab"` button and the panel is
/// `role="tabpanel"`, giving screen-reader users a proper tab widget.
pub fn tabs<Msg: Clone + 'static>(
    active: usize,
    tabs: Vec<(String, TabContent<Msg>)>,
    on_select: impl Fn(usize) -> Msg + 'static,
) -> UITree<Msg> {
    UITree::container(|c| {
        let tablist = c.container(|bar| {
            for (i, (label, _)) in tabs.iter().enumerate() {
                let selected = i == active;
                bar.button(label.clone())
                    .attr("role", "tab")
                    .attr("aria-selected", if selected { "true" } else { "false" })
                    .attr("tabindex", if selected { "0" } else { "-1" })
                    .on_click(on_select(i));
            }
        });
        tablist.attr("role", "tablist");
        if let Some((_, content)) = tabs.into_iter().nth(active) {
            let panel = c.container(|panel| {
                content(panel);
            });
            panel.attr("role", "tabpanel");
        }
    })
}

/// A dropdown / select. `selected` is the index of the chosen item; `on_change`
/// receives the new index. Renders as a `List` of option buttons; the selected
/// option is `aria-selected`. (A native `<select>` is a future backend
/// optimisation — this keeps it backend-agnostic today.)
pub fn dropdown<Msg: Clone + 'static>(
    label: impl Into<String>,
    options: Vec<String>,
    selected: Option<usize>,
    on_change: impl Fn(Option<usize>) -> Msg + 'static,
) -> UITree<Msg> {
    let label = label.into();
    UITree::container(|c| {
        let wrap = c.container(|wrap| {
            wrap.list(|l| {
                for (i, opt) in options.into_iter().enumerate() {
                    let is_sel = selected == Some(i);
                    l.button(opt)
                        .attr("role", "option")
                        .attr("aria-selected", if is_sel { "true" } else { "false" })
                        .on_click(on_change(Some(i)));
                }
            });
        });
        wrap.attr("role", "group").aria("label", label.clone());
    })
}

/// A sortable data table. Renders a [`NodeKind::DataGrid`] whose rows are sorted
/// by `sort_column` (ascending when `asc`, descending otherwise). Clicking a
/// column header toggles the sort via `on_sort`. Purely presentational: the
/// sorting is done here at build time so backends just paint the grid.
pub fn sortable_table<Msg: Clone + 'static>(
    columns: Vec<String>,
    mut rows: Vec<Vec<String>>,
    sort_column: usize,
    asc: bool,
    on_sort: impl Fn(usize) -> Msg + 'static,
) -> UITree<Msg> {
    if sort_column < columns.len() {
        rows.sort_by(|a, b| {
            let av = a.get(sort_column).map(|s| s.as_str()).unwrap_or("");
            let bv = b.get(sort_column).map(|s| s.as_str()).unwrap_or("");
            if asc {
                av.cmp(bv)
            } else {
                bv.cmp(av)
            }
        });
    }
    UITree::container(|c| {
        let wrap = c.container(|wrap| {
            wrap.list(|l| {
                for (i, col) in columns.iter().enumerate() {
                    let is_sorted = i == sort_column;
                    let indicator = if !is_sorted {
                        String::new()
                    } else if asc {
                        " ▲".to_string()
                    } else {
                        " ▼".to_string()
                    };
                    l.button(format!("{col}{indicator}"))
                        .attr("role", "columnheader")
                        .attr(
                            "aria-sort",
                            if !is_sorted {
                                "none"
                            } else if asc {
                                "ascending"
                            } else {
                                "descending"
                            },
                        )
                        .on_click(on_sort(i));
                }
            });
            wrap.data_grid(columns, rows);
        });
        wrap.attr("role", "table");
    })
}

/// A minimal date picker. `value` is the selected `YYYY-MM-DD` string (or
/// empty); `on_change` receives the new value typed into the text input. Pair
/// with a calendar popover in a real design system — this provides the
/// labelled, accessible single-field entry point all backends can render.
pub fn date_picker<Msg: Clone + 'static>(
    label: impl Into<String>,
    value: impl Into<String>,
    on_change: impl Fn(String) -> Msg + Send + Sync + 'static,
) -> UITree<Msg> {
    let label = label.into();
    let value = value.into();
    UITree::container(|c| {
        let wrap = c.container(|wrap| {
            wrap.input(value).attr("type", "date").on_input(on_change);
        });
        wrap.attr("role", "group").aria("label", label.clone());
    })
}

/// Wires a [`Resource`]'s loading region to an `aria-live` announcement, so
/// screen readers are told when an async fetch starts and finishes. Returns an
/// effect handle that must stay alive (forget it) for the lifetime of the
/// region; the closure `announce` is called with the live status text
/// (`"Loading…"`, `"Ready"`, or `"Error: …"`), which the caller typically
/// renders into an `aria-live="polite"` node.
pub fn announce_resource_status<T: Clone + 'static>(
    resource: Resource<T>,
    announce: impl Fn(String) + 'static,
) -> EffectHandle {
    create_effect(move || {
        let status = match resource.state() {
            crate::resource::ResourceState::Loading => "Loading…".to_string(),
            crate::resource::ResourceState::Ready(_) => "Ready".to_string(),
            crate::resource::ResourceState::Error(e) => format!("Error: {e}"),
        };
        announce(status);
    })
}

/// An `aria-live` announcement region builder: renders `text` inside a visually
/// present (or `sr-only`-classed) container with `aria-live="polite"`, the
/// standard hook for assistive-tech status updates.
pub fn live_region<Msg>(text: impl Into<String>, polite: bool) -> UITree<Msg> {
    let live = if polite { "polite" } else { "assertive" };
    UITree::container(|c| {
        c.text(text).attr("aria-live", live).attr("role", "status");
    })
}

/// Focus-management helper for a roving-tabindex group of `count` items. Given
/// the currently-focused index and the direction a user is moving (via arrow
/// keys), it returns the next focus index clamped to `[0, count)`. Pair this
/// with a `keydown` handler that maps ArrowUp/Down/Left/Right to `move_focus`
/// and renders each item with `tabindex = if i == focus { 0 } else { -1 }` —
/// the WAI-ARIA roving-tabindex pattern for composite widgets (menus,
/// tablists, grids).
pub fn move_focus(focus: usize, count: usize, delta: isize) -> usize {
    if count == 0 {
        return 0;
    }
    let next = focus as isize + delta;
    next.clamp(0, (count as isize) - 1) as usize
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::resource::Resource;
    use crate::ui_tree::{NodeKind, NodeMeta};
    use std::rc::Rc;

    fn find_attr(meta: &NodeMeta<()>, name: &str) -> Option<String> {
        meta.attrs
            .iter()
            .find(|(k, _)| k == name)
            .map(|(_, v)| v.clone())
    }

    #[test]
    fn modal_hidden_when_closed() {
        let m = modal::<()>(false, "Title", |_| {}, (), "modal");
        assert!(matches!(m.kind, NodeKind::Container { children } if children.is_empty()));
    }

    #[test]
    fn modal_open_renders_dialog_with_aria() {
        let m = modal::<()>(true, "Settings", |_| {}, (), "modal");
        let NodeKind::Container { children } = &m.kind else {
            panic!("expected root container");
        };
        let NodeKind::Portal { content, .. } = &children[0].kind else {
            panic!("expected portal");
        };
        assert_eq!(find_attr(&content.meta, "role").as_deref(), Some("dialog"));
        assert_eq!(find_attr(&content.meta, "aria-modal").as_deref(), Some("true"));
    }

    #[test]
    fn tabs_mark_active_and_hidden() {
        let t = tabs::<()>(
            1,
            vec![
                ("A".into(), Box::new(|_| {})),
                ("B".into(), Box::new(|_| {})),
            ],
            |_| (),
        );
        let NodeKind::Container { children } = &t.kind else {
            panic!("expected container");
        };
        let NodeKind::Container { children: bar, .. } = &children[0].kind else {
            panic!("expected tablist");
        };
        assert_eq!(find_attr(&bar[0].meta, "aria-selected").as_deref(), Some("false"));
        assert_eq!(find_attr(&bar[1].meta, "aria-selected").as_deref(), Some("true"));
    }

    #[test]
    fn sortable_table_sorts_rows_ascending_and_descending() {
        let rows = vec![
            vec!["Charlie".into(), "3".into()],
            vec!["Alice".into(), "1".into()],
            vec!["Bob".into(), "2".into()],
        ];
        let asc = sortable_table::<()>(
            vec!["Name".into(), "N".into()],
            rows.clone(),
            0,
            true,
            |_| (),
        );
        let NodeKind::Container { children } = &asc.kind else {
            panic!();
        };
        let NodeKind::Container { children: inner } = &children[0].kind else {
            panic!();
        };
        let NodeKind::DataGrid { rows: sorted, .. } = &inner[1].kind else {
            panic!("expected grid");
        };
        assert_eq!(sorted[0][0], "Alice");
        assert_eq!(sorted[2][0], "Charlie");

        let desc = sortable_table::<()>(
            vec!["Name".into(), "N".into()],
            rows,
            0,
            false,
            |_| (),
        );
        let NodeKind::Container { children } = &desc.kind else {
            panic!();
        };
        let NodeKind::Container { children: inner } = &children[0].kind else {
            panic!();
        };
        let NodeKind::DataGrid { rows: sorted, .. } = &inner[1].kind else {
            panic!("expected grid");
        };
        assert_eq!(sorted[0][0], "Charlie");
    }

    #[test]
    fn dropdown_marks_selection() {
        let d = dropdown::<()>("Pick", vec!["x".into(), "y".into()], Some(1), |_| ());
        let NodeKind::Container { children } = &d.kind else {
            panic!();
        };
        let NodeKind::Container { children: wrap } = &children[0].kind else {
            panic!();
        };
        let NodeKind::List { items } = &wrap[0].kind else {
            panic!();
        };
        assert_eq!(find_attr(&items[0].meta, "aria-selected").as_deref(), Some("false"));
        assert_eq!(find_attr(&items[1].meta, "aria-selected").as_deref(), Some("true"));
    }

    #[test]
    fn date_picker_is_labelled_input() {
        let d = date_picker::<()>("Birthday", "2020-01-01", |_| ());
        let NodeKind::Container { children } = &d.kind else {
            panic!();
        };
        let NodeKind::Container { children: wrap } = &children[0].kind else {
            panic!();
        };
        let NodeKind::Input { value } = &wrap[0].kind else {
            panic!("expected input");
        };
        assert_eq!(value, "2020-01-01");
        assert_eq!(find_attr(&wrap[0].meta, "type").as_deref(), Some("date"));
    }

    #[test]
    fn move_focus_clamps_within_bounds() {
        assert_eq!(move_focus(0, 3, -1), 0, "stays at 0");
        assert_eq!(move_focus(1, 3, 1), 2);
        assert_eq!(move_focus(2, 3, 1), 2, "clamps at last");
        assert_eq!(move_focus(0, 3, 2), 2);
    }

    #[test]
    fn announce_resource_status_reacts_to_state() {
        // `Resource::new` with an Err loader settles into Error immediately.
        let r = Resource::<String>::new(|| Err("loading".to_string()));
        let seen = Rc::new(std::cell::RefCell::new(Vec::new()));
        let seen2 = seen.clone();
        let _handle = announce_resource_status(r.clone(), move |s| {
            seen2.borrow_mut().push(s);
        });
        assert_eq!(seen.borrow().last().map(|s| s.as_str()), Some("Error: loading"));
        r.set_result(Ok("done".to_string()));
        assert_eq!(seen.borrow().last().map(|s| s.as_str()), Some("Ready"));
    }

    #[test]
    fn live_region_has_aria_live() {
        let r = live_region::<()>("saved", true);
        let NodeKind::Container { children } = &r.kind else {
            panic!();
        };
        let NodeKind::Text { .. } = &children[0].kind else {
            panic!("expected text");
        };
        assert_eq!(find_attr(&children[0].meta, "aria-live").as_deref(), Some("polite"));
        assert_eq!(find_attr(&children[0].meta, "role").as_deref(), Some("status"));
    }
}
