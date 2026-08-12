mod client_kind;
mod pwa;
mod router;

pub use client_kind::{detect, detect_with, ClientKind};
pub use pwa::{
    manifest, manifest_link, registration_script, service_worker, update_available_script,
    PwaConfig,
};
pub use router::{
    build_router, serve, Command, CommandResponse, CorsPolicy, RateLimitConfig, SmartRouter,
    SmartRouterBuilder,
};
