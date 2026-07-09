mod client_kind;
mod router;

pub use client_kind::ClientKind;
pub use router::{build_router, serve, SmartRouter, SmartRouterBuilder};
