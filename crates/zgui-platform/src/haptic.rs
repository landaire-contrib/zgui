//! Tactile feedback the device plays.

/// A tactile signal, as iOS plays it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Haptic {
    /// A collision of the given weight, as when a menu lifts or a swipe crosses its threshold.
    Impact(ImpactWeight),
    /// A move from one choice to the next.
    Selection,
    /// The outcome of a task.
    Notification(Outcome),
}

/// How heavy a [`Haptic::Impact`] feels.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum ImpactWeight {
    /// A small, light object.
    Light,
    /// A medium object.
    Medium,
    /// A large, heavy object.
    Heavy,
}

/// What a [`Haptic::Notification`] reports.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Outcome {
    /// The task succeeded.
    Success,
    /// The task produced a warning.
    Warning,
    /// The task failed.
    Error,
}
