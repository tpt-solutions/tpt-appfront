use tpt_appfront_core::{create_effect, view, Signal, UITree};
use std::cell::RefCell;
use std::rc::Rc;
use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::*;

#[derive(Debug, Clone)]
enum Msg {
    SetDraft(String),
    AddTask,
    AddSample,
    ClearDone,
    ClearAll,
}

#[derive(Clone)]
struct Task {
    name: String,
    done: bool,
}

#[wasm_bindgen(start)]
pub fn start() -> Result<(), JsValue> {
    console_error_panic_hook::set_once();

    let window = web_sys::window().expect("no window");
    let document = window.document().expect("no document");
    let body = document.body().expect("no body");

    let tasks = Signal::new(Vec::<Task>::new());
    let draft = Signal::new(String::new());
    let remaining = Signal::new(String::from("0 tasks remaining"));

    // The draft input element is captured after mount so `Add task` can clear
    // it. The form panel is mounted once and never re-rendered, so typing in
    // the input never loses focus.
    let input_el: Rc<RefCell<Option<web_sys::HtmlInputElement>>> = Rc::new(RefCell::new(None));

    let dispatch: Rc<dyn Fn(Msg)> = {
        let tasks = tasks.clone();
        let draft = draft.clone();
        let input_el = input_el.clone();
        Rc::new(move |msg| match msg {
            Msg::SetDraft(s) => draft.set(s),
            Msg::AddTask => {
                let text = draft.get().trim().to_string();
                if !text.is_empty() {
                    let mut v = tasks.get();
                    v.push(Task {
                        name: text,
                        done: false,
                    });
                    tasks.set(v);
                    draft.set(String::new());
                    if let Some(el) = input_el.borrow().as_ref() {
                        el.set_value("");
                    }
                }
            }
            Msg::AddSample => {
                let mut v = tasks.get();
                let n = v.len() + 1;
                v.push(Task {
                    name: format!("Sample task {n}"),
                    done: false,
                });
                tasks.set(v);
            }
            Msg::ClearDone => {
                let v: Vec<Task> = tasks.get().into_iter().filter(|t| !t.done).collect();
                tasks.set(v);
            }
            Msg::ClearAll => tasks.set(Vec::new()),
        })
    };

    // ---- form panel (mounted once) ----
    let form = document.create_element("div")?;
    body.append_child(&form)?;

    let draft_for_ui = draft.clone();
    let form_ui: UITree<Msg> = view! {
        <Container class="todo-form">
            <Heading level={1u8}>"Task Board"</Heading>
            <Input value={draft_for_ui.get()} on_input={Msg::SetDraft} />
            <Button on_click={Msg::AddTask}>"Add task"</Button>
            <List class="quick-actions">
                <Button on_click={Msg::AddSample}>"Add sample"</Button>
                <Button on_click={Msg::ClearDone}>"Clear done"</Button>
                <Button on_click={Msg::ClearAll}>"Clear all"</Button>
            </List>
        </Container>
    };
    tpt_appfront_dom::mount(&form, &form_ui, dispatch.clone())?;

    if let Some(el) = form.query_selector("input")? {
        if let Ok(input) = el.dyn_into::<web_sys::HtmlInputElement>() {
            *input_el.borrow_mut() = Some(input);
        }
    }

    // Live "X tasks remaining" line: a reactive text node bound to `remaining`.
    let (remaining_node, remaining_handle) = tpt_appfront_dom::reactive_text(&document, remaining.clone())?;
    form.append_child(&remaining_node)?;
    // Whole-process root mount: forgetting is an explicit choice here, not
    // reactive_text's default behavior.
    std::mem::forget(remaining_handle);

    // Keep `remaining` in sync with the task list without re-mounting the form.
    let tasks_for_remaining = tasks.clone();
    let remaining_for_effect = remaining.clone();
    let rem_handle = create_effect(move || {
        let v = tasks_for_remaining.get();
        let total = v.len();
        let done = v.iter().filter(|t| t.done).count();
        remaining_for_effect.set(format!(
            "{} task{} total, {} remaining",
            total,
            if total == 1 { "" } else { "s" },
            total - done
        ));
    });
    std::mem::forget(rem_handle);

    // ---- task data panel (re-mounted whenever `tasks` changes) ----
    let panel = document.create_element("div")?;
    body.append_child(&panel)?;

    let render_panel: Rc<dyn Fn()> = {
        let tasks = tasks.clone();
        let panel = panel.clone();
        let dispatch = dispatch.clone();
        Rc::new(move || {
            let v = tasks.get();
            let rows: Vec<Vec<String>> = v
                .iter()
                .map(|t| {
                    vec![
                        t.name.clone(),
                        if t.done {
                            "Done".to_string()
                        } else {
                            "Pending".to_string()
                        },
                    ]
                })
                .collect();
            let grid_ui: UITree<Msg> = view! {
                <Container class="todo-board">
                    <DataGrid
                        columns={vec!["Task".to_string(), "Status".to_string()]}
                        rows={rows}
                    />
                </Container>
            };
            panel.set_inner_html("");
            let _ = tpt_appfront_dom::mount(&panel, &grid_ui, dispatch.clone());
        })
    };

    let tasks_for_panel = tasks.clone();
    let render_panel_for_effect = render_panel.clone();
    let panel_handle = create_effect(move || {
        tasks_for_panel.get(); // track `tasks`
        render_panel_for_effect();
    });
    std::mem::forget(panel_handle);

    Ok(())
}
