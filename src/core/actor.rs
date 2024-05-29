use std::fmt::Debug;
use std::sync::Arc;

use async_trait::async_trait;
use tokio::sync::Mutex;

use crate::core::actor::actor_context::ActorContext;
use crate::core::actor::actor_path::ActorPath;
use crate::core::actor::actor_ref::UntypedActorRef;
use crate::core::actor::actor_system::ActorSystem;
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
pub mod props;

#[async_trait]
pub trait Actor: Debug + Send + Sync {
  type M: Message;

  async fn around_pre_start(&mut self, ctx: ActorContext<Self::M>) {
    self.pre_start(ctx).await;
  }
  async fn pre_start(&mut self, ctx: ActorContext<Self::M>);

  async fn around_post_stop(&mut self, ctx: ActorContext<Self::M>) {
    self.post_stop(ctx).await;
  }
  async fn post_stop(&mut self, ctx: ActorContext<Self::M>) {}

  async fn receive(&mut self, ctx: ActorContext<Self::M>, message: Self::M);
}

#[async_trait]
pub trait AnyActor: Debug + Send + Sync {
  fn path(&self) -> &ActorPath;

  async fn set_parent(&mut self, parent_ref: UntypedActorRef);
  async fn get_parent(&self) -> Option<UntypedActorRef>;
  async fn add_child(&self, child_cell: AnyActorArc);
  async fn get_children(&self) -> Vec<AnyActorArc>;
  async fn send_message(&self, message: AnyMessage) -> Result<(), QueueError<AnyMessage>>;
  async fn send_system_message(&self, system_message: SystemMessage) -> Result<(), QueueError<SystemMessage>>;

  async fn start(&self) -> Result<(), QueueError<SystemMessage>>;
  async fn stop(&self) -> Result<(), QueueError<SystemMessage>>;
  async fn suspend(&self) -> Result<(), QueueError<SystemMessage>>;
  async fn resume(&self) -> Result<(), QueueError<SystemMessage>>;

  async fn child_terminated(&mut self, child: UntypedActorRef);

  async fn invoke(&mut self, message: AnyMessage);
  async fn system_invoke(&mut self, system_message: SystemMessage);
}

pub type AnyActorArc = Arc<Mutex<Box<dyn AnyActor>>>;

#[async_trait::async_trait]
pub trait AnyActorRef: Debug + PartialEq {
  fn path(&self) -> &ActorPath;
  async fn tell_any(&self, system: &ActorSystem, message: AnyMessage);
}

#[async_trait::async_trait]
pub trait SysTell: AnyActorRef {
  async fn sys_tell(&self, system: &ActorSystem, message: SystemMessage);
  async fn start(&mut self, system: &ActorSystem) {
    self.sys_tell(system, SystemMessage::Create).await;
  }
  async fn stop(&mut self, system: &ActorSystem) {
    self.sys_tell(system, SystemMessage::Terminate).await;
  }
  async fn suspend(&mut self, system: &ActorSystem) {
    self.sys_tell(system, SystemMessage::Suspend).await;
  }
  async fn resume(&mut self, system: &ActorSystem) {
    self.sys_tell(system, SystemMessage::Resume).await;
  }
}
