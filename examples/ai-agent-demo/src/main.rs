//! Demonstrates the Phase 10 programmatic AI agent API end to end: a plain
//! Rust "agent" loop (standing in for an LLM tool-calling loop) discovers
//! actions via [`tpt_appfront_core::query_state`], picks one, and drives the UI
//! via [`tpt_appfront_core::trigger_event`] — no browser, no canvas, headless.
//! Also prints the JSON-LD / AI-Schema representation an AI agent hitting
//! `GET /ai-schema.json` on `tpt-appfront-server` would receive.
use tpt_appfront_core::{navigate_to, query_state, trigger_event, ContainerBuilder, Signal, UITree};

#[derive(Debug, Clone)]
enum Msg {
    AddTask,
    RemoveLast,
}

fn build_ui(tasks: &Signal<Vec<String>>) -> UITree<Msg> {
    let items = tasks.get();
    UITree::container(|c: &mut ContainerBuilder<Msg>| {
        c.heading(1, "Task List").class("title");
        c.text(format!("{} task(s)", items.len()));
        c.button("Add Task")
            .on_click(Msg::AddTask)
            .ai_action("add_task")
            .ai_description("Append a new task to the list");
        if !items.is_empty() {
            c.button("Remove Last")
                .on_click(Msg::RemoveLast)
                .ai_action("remove_last")
                .ai_description("Remove the most recently added task");
        }
        c.list(|l| {
            for task in &items {
                l.text(task.clone());
            }
        });
    })
}

fn dispatch(tasks: &Signal<Vec<String>>, msg: Msg) {
    match msg {
        Msg::AddTask => {
            let mut items = tasks.get();
            items.push(format!("Task {}", items.len() + 1));
            tasks.set(items);
        }
        Msg::RemoveLast => {
            let mut items = tasks.get();
            items.pop();
            tasks.set(items);
        }
    }
}

/// Stands in for an LLM's tool-selection step: given the discovered
/// interactive elements, pick which `ai_action` to invoke next.
fn agent_choose_action(state: &tpt_appfront_core::AgentState, step: usize) -> Option<String> {
    if step < 3 {
        // Prefer add_task for the first few steps to grow the list...
        state
            .interactive_elements
            .iter()
            .find(|e| e.action.as_deref() == Some("add_task"))
            .and_then(|e| e.action.clone())
    } else {
        // ...then remove_last, if it's available.
        state
            .interactive_elements
            .iter()
            .find(|e| e.action.as_deref() == Some("remove_last"))
            .and_then(|e| e.action.clone())
    }
}

fn main() {
    navigate_to("/tasks");

    let tasks = Signal::new(Vec::<String>::new());

    println!("=== Agent loop driving the UI headlessly ===");
    for step in 0..4 {
        let mut ui = build_ui(&tasks);
        ui.assign_ids();

        let state = query_state(&ui);
        println!(
            "\nstep {step}: route={} available_actions={:?}",
            state.current_route,
            state
                .interactive_elements
                .iter()
                .filter_map(|e| e.action.clone())
                .collect::<Vec<_>>()
        );

        match agent_choose_action(&state, step) {
            Some(action) => {
                let tasks_for_dispatch = tasks.clone();
                let dispatched = trigger_event(&ui, &action, &|msg| {
                    dispatch(&tasks_for_dispatch, msg)
                });
                println!("  agent invoked `{action}` -> dispatched={dispatched}");
            }
            None => println!("  agent found no matching action, stopping"),
        }
    }

    println!("\nfinal tasks: {:?}", tasks.get());

    println!("\n=== What GET /ai-schema.json would return for the final UI ===");
    let mut final_ui = build_ui(&tasks);
    final_ui.assign_ids();
    let (json_ld, ai_schema) = tpt_appfront_ai_schema::both(&final_ui);
    println!(
        "JSON-LD:\n{}",
        serde_json::to_string_pretty(&json_ld).unwrap()
    );
    println!(
        "AI Schema:\n{}",
        serde_json::to_string_pretty(&ai_schema).unwrap()
    );
}
