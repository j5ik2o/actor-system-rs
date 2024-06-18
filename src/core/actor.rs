use std::error::Error;
use std::fmt::Debug;
use std::sync::Arc;

use tokio::sync::{Mutex, Notify};

use crate::core::actor::actor_context::{ActorContext, ActorContextRef};
use crate::core::actor::actor_path::ActorPath;
use crate::core::actor::actor_ref::{InternalActorRef, LocalActorRef};
use crate::core::actor::supervisor_strategy::{OneForOneStrategy, SupervisorStrategy, RESTART_DECIDER};
use crate::core::dispatch::any_message::AnyMessage;
use crate::core::dispatch::mailbox::system_message::SystemMessage;
use crate::core::dispatch::message::Message;
use crate::core::util::queue::QueueError;

pub mod actor_cell;
pub mod actor_context;
pub mod actor_path;
pub mod actor_ref;
pub mod actor_system;
pub mod address;
mod death_watch;
pub mod props;
pub mod supervisor_strategy;
mod typed_actor_context;

pub type ActorError = Box<dyn Error + Send + Sync + 'static>;

#[async_trait::async_trait]
pub trait Actor: Debug + Send + Sync {
  type M: Message;

  async fn supervisor_strategy(&self) -> Arc<Box<dyn SupervisorStrategy>> {
    Arc::new(Box::new(OneForOneStrategy::with_decider(RESTART_DECIDER.clone())))
  }

  async fn around_pre_start(&mut self, ctx: ActorContext) {
    self.pre_start(ctx).await;
  }
  async fn pre_start(&mut self, ctx: ActorContext) {
    log::debug!("pre_start: {}", ctx.self_path().await);
  }

  async fn child_terminated(&mut self, ctx: ActorContext, child: InternalActorRef) {
    log::debug!("child_terminated: {}, {}", ctx.self_path().await, child.path());
  }

  async fn all_children_terminated(&mut self, ctx: ActorContext) {
    log::debug!("all_children_terminated: {}", ctx.self_path().await);
  }

  async fn around_post_stop(&mut self, ctx: ActorContext) {
    self.post_stop(ctx).await;
  }

  async fn post_stop(&mut self, ctx: ActorContext) {
    log::debug!("post_stop: {}", ctx.self_path().await)
  }

  async fn receive(&mut self, ctx: ActorContext, message: Self::M) -> Result<(), ActorError>;
}

#[async_trait::async_trait]
pub trait AnyActorWriter: Debug + Send + Sync {
  fn set_actor_context_ref(&mut self, actor_cells: ActorContextRef);
  async fn path(&self) -> ActorPath;

  async fn get_parent(&self) -> Option<InternalActorRef>;
  async fn get_children(&self) -> Vec<AnyActorWriterArc>;
  async fn send_message(&self, message: AnyMessage) -> Result<(), QueueError<AnyMessage>>;
  async fn send_system_message(&self, system_message: SystemMessage) -> Result<(), QueueError<SystemMessage>>;

  async fn start(&self) -> Result<(), QueueError<SystemMessage>>;
  async fn stop(&self) -> Result<(), QueueError<SystemMessage>>;
  async fn suspend(&self) -> Result<(), QueueError<SystemMessage>>;
  async fn resume(&self, cause: Arc<ActorError>) -> Result<(), QueueError<SystemMessage>>;

  async fn get_terminate_notify(&self) -> Arc<Notify>;
}

#[async_trait::async_trait]
pub trait AnyActorReader: Debug + Send + Sync {
  fn set_actor_context_ref(&mut self, actor_cells: ActorContextRef);
  async fn path(&self) -> ActorPath;
  async fn get_parent(&self) -> Option<InternalActorRef>;

  async fn child_terminated(&mut self, child: InternalActorRef);

  async fn invoke(&mut self, message: AnyMessage);
  async fn system_invoke(&mut self, system_message: SystemMessage);

  fn supervisor_strategy(&self) -> Arc<Box<dyn SupervisorStrategy>>;

  async fn get_terminate_notify(&self) -> Arc<Notify>;
}

pub type AnyActorWriterArc = Arc<Mutex<Box<dyn AnyActorWriter>>>;
pub type AnyActorReaderArc = Arc<Mutex<Box<dyn AnyActorReader>>>;

#[async_trait::async_trait]
pub trait AnyActorRef: Debug + PartialEq {
  fn path(&self) -> &ActorPath;
  async fn tell_any(&self, message: AnyMessage);
}

#[async_trait::async_trait]
pub trait SysTell: AnyActorRef {
  async fn sys_tell(&self, message: SystemMessage);
  async fn start(&mut self) {
    self.sys_tell(SystemMessage::Create).await;
  }
  async fn stop(&mut self) {
    self.sys_tell(SystemMessage::Terminate).await;
  }
  async fn suspend(&mut self) {
    self.sys_tell(SystemMessage::Suspend).await;
  }
  async fn resume(&mut self, cause: Arc<ActorError>) {
    self
      .sys_tell(SystemMessage::Resume {
        caused_by_failure: cause,
      })
      .await;
  }
  async fn restart(&mut self, cause: Arc<ActorError>) {
    self.sys_tell(SystemMessage::Recreate { cause }).await;
  }

  async fn when_terminated(&self);
}
