use crate::core::actor::child_restart_stats::ChildRestartStats;

#[derive(Debug, Clone)]
pub enum ChildState {
  ChildNameReserved,
  ChildRestartStats(ChildRestartStats),
}
impl PartialEq for ChildState {
  fn eq(&self, other: &Self) -> bool {
    match (self, other) {
      (Self::ChildNameReserved, Self::ChildNameReserved) => true,
      (Self::ChildRestartStats(l), Self::ChildRestartStats(r)) => l == r,
      _ => false,
    }
  }
}

impl ChildState {
  pub fn as_child_restart_stats(&self) -> Option<&ChildRestartStats> {
    match self {
      Self::ChildRestartStats(stats) => Some(stats),
      _ => None,
    }
  }
}
