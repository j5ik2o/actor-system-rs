use std::sync::Arc;
use std::time::Duration;

use once_cell::sync::Lazy;

use crate::core::actor::actor_context::ActorContext;
use crate::core::actor::actor_ref::UntypedActorRef;
use crate::core::actor::{ActorError, SysTell};

#[derive(Debug, Clone)]
pub enum Directive {
  Resume,
  Restart,
  Stop,
  Escalate,
}

pub const RESTART_DECIDER: Lazy<DeciderArc> = Lazy::new(|| Arc::new(Box::new(|_: Arc<ActorError>| Directive::Restart)));
pub const STOP_DECIDER: Lazy<DeciderArc> = Lazy::new(|| Arc::new(Box::new(|_: Arc<ActorError>| Directive::Stop)));

pub type DeciderArc = Arc<Box<dyn Fn(Arc<ActorError>) -> Directive + Send + Sync + 'static>>;

#[async_trait::async_trait]
pub trait SupervisorStrategy: Send + Sync + 'static {
  fn decider(&self) -> DeciderArc;
  async fn handle_child_terminated(&self, ctx: ActorContext, child: UntypedActorRef, children: Vec<UntypedActorRef>);

  async fn process_failure(
    &self,
    ctx: ActorContext,
    restart: bool,
    child: UntypedActorRef,
    cause: Arc<ActorError>,
    children: Vec<UntypedActorRef>,
  );

  async fn handle_failure(
    &self,
    ctx: ActorContext,
    _restart: bool,
    mut child: UntypedActorRef,
    cause: Arc<ActorError>,
    children: Vec<UntypedActorRef>,
  ) -> bool {
    let directive = self.decider()(cause.clone());
    match directive {
      Directive::Resume => {
        child.resume(cause).await;
        true
      }
      Directive::Restart => {
        self.process_failure(ctx, true, child, cause, children).await;
        true
      }
      Directive::Stop => {
        self.process_failure(ctx, false, child, cause, children).await;
        true
      }
      Directive::Escalate => false,
    }
  }
}

pub struct OneForOneStrategy {
  max_nr_of_retries: i32,
  within_time_range: Duration,
  logging_enabled: bool,
  decider: DeciderArc,
}

impl OneForOneStrategy {
  pub fn new(max_nr_of_retries: i32, within_time_range: Duration, logging_enabled: bool, decider: DeciderArc) -> Self {
    Self {
      max_nr_of_retries,
      within_time_range,
      logging_enabled,
      decider,
    }
  }

  pub fn with_decider(decider: DeciderArc) -> Self {
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
  fn decider(&self) -> DeciderArc {
    self.decider.clone()
  }

  async fn handle_child_terminated(&self, _ctx: ActorContext, _child: UntypedActorRef, _children: Vec<UntypedActorRef>) {}

  async fn process_failure(
    &self,
    _ctx: ActorContext,
    restart: bool,
    mut child: UntypedActorRef,
    cause: Arc<ActorError>,
    _children: Vec<UntypedActorRef>,
  ) {
    if restart {
      child.restart(cause).await
    } else {
      child.stop().await
    }
  }
}

unsafe impl Send for OneForOneStrategy {}
unsafe impl Sync for OneForOneStrategy {}

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
