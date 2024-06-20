use std::time::Duration;

use tokio::time::Instant;

use crate::core::actor::actor_path::ActorPathBehavior;
use crate::core::actor::actor_ref::InternalActorRef;
use crate::core::actor::AnyActorRef;

#[derive(Debug, Clone)]
pub struct ChildRestartStats {
  child: InternalActorRef,
  max_nr_of_retries_count: i32,
  restart_time_window_start_nanos: i64,
}

impl PartialEq for ChildRestartStats {
  fn eq(&self, other: &Self) -> bool {
    self.child == other.child
  }
}

impl ChildRestartStats {
  pub fn new(child: InternalActorRef) -> Self {
    Self {
      child,
      max_nr_of_retries_count: 0,
      restart_time_window_start_nanos: 0,
    }
  }

  pub fn with_child(mut self, child: InternalActorRef) -> Self {
    self.child = child;
    self
  }

  pub fn with_max_nr_of_retries_count(mut self, max_nr_of_retries_count: i32) -> Self {
    self.max_nr_of_retries_count = max_nr_of_retries_count;
    self
  }

  pub fn with_restart_time_window_start_nanos(mut self, restart_time_window_start_nanos: i64) -> Self {
    self.restart_time_window_start_nanos = restart_time_window_start_nanos;
    self
  }

  pub fn child(&self) -> InternalActorRef {
    self.child.clone()
  }

  pub async fn uid(&self) -> u32 {
    self.child.path().uid()
  }

  pub fn request_restart_permission(&mut self, retries_window: (Option<i32>, Option<i32>)) -> bool {
    match retries_window {
      (Some(retires), _) if retires < 1 => false,
      (Some(retires), None) => {
        self.max_nr_of_retries_count += 1;
        self.max_nr_of_retries_count <= retires
      }
      (x, Some(window)) => self.retries_in_window_okay(if x.is_some() { x.unwrap() } else { 1 }, window),
      (None, _) => true,
    }
  }

  fn retries_in_window_okay(&mut self, retries: i32, window: i32) -> bool {
    let retries_done = self.max_nr_of_retries_count + 1;
    let now = Instant::now().elapsed().as_nanos() as i64;
    let window_start = if self.restart_time_window_start_nanos == 0 {
      self.restart_time_window_start_nanos = now;
      now
    } else {
      self.restart_time_window_start_nanos
    };
    let inside_window = (now - window_start) as u128 <= Duration::from_millis(window as u64).as_nanos();
    if inside_window {
      self.max_nr_of_retries_count = retries_done;
      retries_done <= retries
    } else {
      self.max_nr_of_retries_count = 1;
      self.restart_time_window_start_nanos = now;
      true
    }
  }
}
