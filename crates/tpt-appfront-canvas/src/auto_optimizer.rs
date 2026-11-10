//! Runtime frame-time profiler that recommends whether to window long lists.
//!
//! `AutoOptimizer` is deliberately pure and headless-testable: it only ever
//! sees `f64` milliseconds, so it behaves identically on native and wasm and
//! needs no renderer internals. A canvas app feeds it the measured per-frame
//! duration of each frame, and reads back the recommended `OptimizerState` to
//! decide whether to apply `VirtualScroll` to unconfigured lists (see
//! `CanvasApp::auto_optimize`).
//!
//! The optimizer only covers virtual scrolling. An earlier `texture_caching`
//! flag was removed: the canvas has no texture cache to toggle, so it could
//! only ever be advisory.
//!
//! Toggling uses a smoothed (EMA) frame time plus a hysteresis counter so a
//! single transient hitch doesn't flap the flag on and off.

/// Which expensive optimizations are currently recommended. Cheap to copy;
/// the canvas app reads this every frame and applies the diff.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OptimizerState {
    /// Window long lists via the `NodeMeta::virtual_scroll` primitive instead
    /// of materializing every row (only worth it when the frame budget is
    /// tight — it adds per-frame slice math).
    pub virtual_scrolling: bool,
}

/// What changed on the most recent `record_frame` call, so the app only acts
/// when something actually flipped.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OptimizerChange {
    None,
    EnabledVirtualScrolling,
    DisabledVirtualScrolling,
}

impl OptimizerChange {
    pub fn is_some(self) -> bool {
        self != OptimizerChange::None
    }
}

pub struct AutoOptimizer {
    /// Smoothed frame time in milliseconds.
    ema_ms: f64,
    /// EMA weighting for the newest sample (0..1). Higher = reacts faster.
    alpha: f64,
    frame_count: u64,
    /// Smoothed frame time above this (ms) is treated as "too slow" → enable
    /// optimizations.
    slow_threshold_ms: f64,
    /// Smoothed frame time below this (ms) is treated as "comfortably fast"
    /// → optimizations may be disabled to save memory/complexity.
    fast_threshold_ms: f64,
    /// Consecutive frames past a threshold required before toggling (hysteresis).
    hold_frames: u64,
    slow_streak: u64,
    fast_streak: u64,
    pub recommendations: OptimizerState,
}

impl Default for AutoOptimizer {
    fn default() -> Self {
        // 60fps budget is ~16.67ms. Enable virtual scrolling once we're sustained
        // above ~20ms (under ~50fps) and drop it once we're comfortably under ~12ms.
        Self::with_thresholds(20.0, 12.0)
    }
}

impl AutoOptimizer {
    /// Builds an optimizer with the given "too slow" / "comfortably fast"
    /// thresholds (in milliseconds) and default smoothing/hysteresis.
    pub fn with_thresholds(slow_threshold_ms: f64, fast_threshold_ms: f64) -> Self {
        assert!(
            fast_threshold_ms < slow_threshold_ms,
            "fast threshold must be below the slow threshold"
        );
        AutoOptimizer {
            ema_ms: 0.0,
            alpha: 0.2,
            frame_count: 0,
            slow_threshold_ms,
            fast_threshold_ms,
            hold_frames: 30,
            slow_streak: 0,
            fast_streak: 0,
            recommendations: OptimizerState {
                virtual_scrolling: false,
            },
        }
    }

    /// Current smoothed frame time (ms) — handy for an on-screen FPS overlay.
    pub fn smoothed_ms(&self) -> f64 {
        self.ema_ms
    }

    pub fn recommendations(&self) -> OptimizerState {
        self.recommendations
    }

    /// Feed the duration of one rendered frame (milliseconds). Returns what
    /// (if anything) changed in the recommendations so the caller can react.
    pub fn record_frame(&mut self, frame_ms: f64) -> OptimizerChange {
        if !frame_ms.is_finite() || frame_ms < 0.0 {
            return OptimizerChange::None;
        }
        if self.frame_count == 0 {
            self.ema_ms = frame_ms;
        } else {
            self.ema_ms = self.alpha * frame_ms + (1.0 - self.alpha) * self.ema_ms;
        }
        self.frame_count += 1;

        if self.ema_ms > self.slow_threshold_ms {
            self.slow_streak += 1;
            self.fast_streak = 0;
        } else if self.ema_ms < self.fast_threshold_ms {
            self.fast_streak += 1;
            self.slow_streak = 0;
        } else {
            // In the dead zone between thresholds: don't accumulate either
            // streak, so toggles only happen on a clear, sustained trend.
            self.slow_streak = 0;
            self.fast_streak = 0;
            return OptimizerChange::None;
        }

        if self.slow_streak >= self.hold_frames && !self.recommendations.virtual_scrolling {
            self.recommendations.virtual_scrolling = true;
            return OptimizerChange::EnabledVirtualScrolling;
        }
        if self.fast_streak >= self.hold_frames && self.recommendations.virtual_scrolling {
            self.recommendations.virtual_scrolling = false;
            return OptimizerChange::DisabledVirtualScrolling;
        }
        OptimizerChange::None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_constructed_with_reasonable_thresholds() {
        let opt = AutoOptimizer::default();
        assert!(opt.slow_threshold_ms > opt.fast_threshold_ms);
        assert!(!opt.recommendations.virtual_scrolling);
    }

    #[test]
    fn sustained_slow_frames_enable_virtual_scrolling() {
        let mut opt = AutoOptimizer::with_thresholds(20.0, 12.0);
        // 50ms frames: way over the slow threshold. Need `hold_frames` (30)
        // consecutive to toggle, so the first flip happens after ~30 frames.
        let mut first_change: Option<OptimizerChange> = None;
        for _ in 0..60 {
            let c = opt.record_frame(50.0);
            if c.is_some() && first_change.is_none() {
                first_change = Some(c);
            }
        }
        assert_eq!(first_change, Some(OptimizerChange::EnabledVirtualScrolling));
        assert!(opt.recommendations.virtual_scrolling);
    }

    #[test]
    fn hysteresis_prevents_flapping_on_a_single_slow_frame() {
        let mut opt = AutoOptimizer::with_thresholds(20.0, 12.0);
        // A long run of fast frames could only *disable* things, but we start
        // with nothing enabled, so a lone slow frame in a sea of fast ones
        // must not enable anything.
        let mut any_change = false;
        for _ in 0..100 {
            let c = opt.record_frame(1.0);
            if c.is_some() {
                any_change = true;
            }
        }
        let c = opt.record_frame(500.0);
        assert!(!c.is_some());
        assert!(!any_change);
        assert!(!opt.recommendations.virtual_scrolling);
    }

    #[test]
    fn sustained_fast_frames_disable_virtual_scrolling() {
        let mut opt = AutoOptimizer::with_thresholds(20.0, 12.0);
        for _ in 0..60 {
            opt.record_frame(50.0);
        }
        assert!(opt.recommendations.virtual_scrolling);

        let mut saw_vscroll_off = false;
        for _ in 0..80 {
            if opt.record_frame(1.0) == OptimizerChange::DisabledVirtualScrolling {
                saw_vscroll_off = true;
            }
        }
        assert!(saw_vscroll_off);
        assert!(!opt.recommendations.virtual_scrolling);
    }

    #[test]
    fn non_finite_samples_are_ignored() {
        let mut opt = AutoOptimizer::default();
        assert_eq!(opt.record_frame(f64::NAN), OptimizerChange::None);
        assert_eq!(opt.record_frame(f64::INFINITY), OptimizerChange::None);
        assert_eq!(opt.record_frame(-1.0), OptimizerChange::None);
    }
}
