use crate::core::actor::actor_context::ActorContext;
use crate::core::actor::actor_ref::UntypedActorRef;
use crate::core::actor::SysTell;
use std::error::Error;
use std::sync::Arc;
use std::time::Duration;

#[derive(Debug, Clone)]
enum Directive {
  Resume,
  Restart,
  Stop,
  Escalate,
}

type Decider = Arc<Box<dyn Fn(Arc<Box<dyn Error + Send + Sync>>) -> Directive>>;

#[async_trait::async_trait]
pub trait SupervisorStrategy {
  fn decider(&self) -> Decider;
  async fn handle_child_terminated(&self, ctx: ActorContext, child: UntypedActorRef, children: Vec<UntypedActorRef>);

  async fn process_failure(
    &self,
    ctx: ActorContext,
    restart: bool,
    child: UntypedActorRef,
    cause: Arc<Box<dyn Error + Send + Sync>>,
    children: Vec<UntypedActorRef>,
  );
}

pub struct OneForOneStrategy {
  max_nr_of_retries: i32,
  within_time_range: Duration,
  logging_enabled: bool,
  decider: Decider,
}

impl OneForOneStrategy {
  pub fn new(max_nr_of_retries: i32, within_time_range: Duration, logging_enabled: bool, decider: Decider) -> Self {
    Self {
      max_nr_of_retries,
      within_time_range,
      logging_enabled,
      decider,
    }
  }

  pub fn with_decider(decider: Decider) -> Self {
    Self::new(-1, Duration::from_millis(u64::MAX), true, decider)
  }

  pub fn with_max_nr_of_retries(mut self, max_nr_of_retries: i32) -> Self {
    self.max_nr_of_retries = max_nr_of_retries;
    self
  }

  fn retries_window(&self) -> (Option<i32>, Option<u128>) {
    (
      max_nr_of_retries_option(self.max_nr_of_retries),
      within_time_range_option(self.within_time_range).map(|d| d.as_millis()),
    )
  }
}

#[async_trait::async_trait]
impl SupervisorStrategy for OneForOneStrategy {
  fn decider(&self) -> Decider {
    self.decider.clone()
  }

  async fn handle_child_terminated(&self, ctx: ActorContext, child: UntypedActorRef, children: Vec<UntypedActorRef>) {}

  async fn process_failure(
    &self,
    ctx: ActorContext,
    restart: bool,
    mut child: UntypedActorRef,
    cause: Arc<Box<dyn Error + Send + Sync>>,
    children: Vec<UntypedActorRef>,
  ) {
    if restart {
      child.restart(cause).await
    } else {
      child.stop().await
    }
  }
}

fn max_nr_of_retries_option(max_nr_of_retries: i32) -> Option<i32> {
  if max_nr_of_retries < 0 {
    None
  } else {
    Some(max_nr_of_retries)
  }
}

fn within_time_range_option(within_time_range: Duration) -> Option<Duration> {
  if within_time_range.as_secs() == u64::MAX && within_time_range.as_secs() == 0 {
    Some(within_time_range)
  } else {
    None
  }
}
