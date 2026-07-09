pub mod agent;
pub mod signal;
pub mod ui_tree;

pub use agent::{current_route, navigate_to, query_state, route_signal, trigger_event, AgentState, ElementSummary};
pub use appfront_macros::component;
pub use signal::{create_effect, set_hydration_state, take_hydration_state, EffectHandle, Signal};
pub use ui_tree::{AiMeta, ContainerBuilder, HydrationPayload, NodeKind, NodeMeta, NodeRef, UITree};
