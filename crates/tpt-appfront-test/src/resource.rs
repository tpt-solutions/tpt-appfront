//! A deterministic driver for [`Resource`] loads in tests.
//!
//! Real loads run on an executor the test does not control. This driver keeps the same rules
//! (the generation check that cancels superseded loads) but makes each completion explicit. A
//! test starts a load, receives a [`Ticket`], and decides when and with what result it completes.
//! That covers the cases that matter: a stale result must not overwrite a newer one, and a
//! reload must put the resource back into `Loading`.
//!
//! ```ignore
//! let mut d = ResourceDriver::new(Resource::<String>::ready("old".into()));
//! let first = d.start();
//! let second = d.start();
//! assert!(!d.complete(first, Ok("stale".into())));  // superseded: dropped
//! assert!(d.complete(second, Ok("fresh".into())));
//! assert_eq!(d.resource().ready_value(), Some("fresh".to_string()));
//! ```

use tpt_appfront_core::{Resource, ResourceState};

/// A load started by [`ResourceDriver::start`]. Completing it is allowed only while it is current.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Ticket(u64);

/// Drives a [`Resource`] through explicit starts and completions, and records every state the
/// resource was observed in.
pub struct ResourceDriver<T> {
    resource: Resource<T>,
    history: Vec<ResourceState<T>>,
}

impl<T: Clone + PartialEq + std::fmt::Debug + 'static> ResourceDriver<T> {
    /// Wraps `resource`, recording its current state as the first history entry.
    pub fn new(resource: Resource<T>) -> Self {
        let history = vec![resource.state()];
        ResourceDriver { resource, history }
    }

    /// The wrapped resource, for handing to the code under test.
    pub fn resource(&self) -> &Resource<T> {
        &self.resource
    }

    /// Starts a load: the resource goes to `Loading`, and earlier tickets stop being current.
    pub fn start(&mut self) -> Ticket {
        self.resource.reload();
        self.record();
        Ticket(self.resource.generation())
    }

    /// Completes a load. The result is committed only if `ticket` is still current. Returns
    /// whether it was committed. A stale completion is dropped, as the real cancellation path
    /// would drop it.
    pub fn complete(&mut self, ticket: Ticket, result: Result<T, String>) -> bool {
        if !self.resource.is_current(ticket.0) {
            return false;
        }
        self.resource.set_result(result);
        self.record();
        true
    }

    /// Every state observed so far, oldest first. Repeated identical states are kept, so a test
    /// can assert how many times a state was reached.
    pub fn history(&self) -> &[ResourceState<T>] {
        &self.history
    }

    /// The current state.
    pub fn state(&self) -> ResourceState<T> {
        self.resource.state()
    }

    /// Panics unless the resource is `Ready` with `expected`.
    pub fn assert_ready(&self, expected: &T) {
        match self.state() {
            ResourceState::Ready(ref v) if v == expected => {}
            other => panic!("expected Ready({expected:?}), found {other:?}"),
        }
    }

    /// Panics unless the resource is `Loading`.
    pub fn assert_loading(&self) {
        let state = self.state();
        assert!(
            matches!(state, ResourceState::Loading),
            "expected Loading, found {state:?}"
        );
    }

    fn record(&mut self) {
        let state = self.resource.state();
        self.history.push(state);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_stale_completion_is_dropped_and_a_current_one_is_committed() {
        let mut d = ResourceDriver::new(Resource::<String>::ready("old".into()));
        let first = d.start();
        let second = d.start();
        assert!(!d.complete(first, Ok("stale".into())));
        assert!(d.complete(second, Ok("fresh".into())));
        d.assert_ready(&"fresh".to_string());
    }

    #[test]
    fn a_load_moves_through_loading_to_ready_and_is_recorded() {
        let mut d = ResourceDriver::new(Resource::<u32>::ready(1));
        let t = d.start();
        d.assert_loading();
        d.complete(t, Ok(2));
        let states: Vec<_> = d.history().to_vec();
        assert_eq!(
            states,
            vec![
                ResourceState::Ready(1),
                ResourceState::Loading,
                ResourceState::Ready(2)
            ]
        );
    }

    #[test]
    fn an_error_result_is_committed_as_an_error_state() {
        let mut d = ResourceDriver::new(Resource::<u32>::ready(1));
        let t = d.start();
        assert!(d.complete(t, Err("offline".into())));
        assert_eq!(d.state(), ResourceState::Error("offline".into()));
    }

    #[test]
    #[should_panic(expected = "expected Ready")]
    fn assert_ready_fails_while_loading() {
        let mut d = ResourceDriver::new(Resource::<u32>::ready(1));
        d.start();
        d.assert_ready(&1);
    }
}
