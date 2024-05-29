use std::fmt::Debug;
use std::sync::Arc;

use async_trait::async_trait;
use tokio::sync::Mutex;

use crate::core::actor::actor_cells::ActorCells;
use crate::core::actor::actor_path::ActorPath;
use crate::core::actor::actor_ref::UntypedActorRef;
use crate::core::dispatch::any_message::AnyMessage;
use crate::core::dispatch::mailbox::system_message::SystemMessage;
use crate::core::dispatch::message::Message;
use crate::core::util::queue::QueueError;

pub mod actor_cell;
pub mod actor_path;
pub mod actor_ref;
pub mod actor_system;
pub mod address;
pub mod props;
pub mod actor_cells;

#[async_trait]
pub trait Actor: Debug + Send + Sync {
  type M: Message;

  async fn around_pre_start(&mut self, ctx: ActorCells) {
    self.pre_start(ctx).await;
  }
  async fn pre_start(&mut self, ctx: ActorCells);

  async fn around_post_stop(&mut self, ctx: ActorCells) {
    self.post_stop(ctx).await;
  }
  async fn post_stop(&mut self, ctx: ActorCells) {}

  async fn receive(&mut self, ctx: ActorCells, message: Self::M);
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

  async fn child_terminated(&mut self, actor_cells: ActorCells, child: UntypedActorRef);

  async fn invoke(&mut self, actor_cells: ActorCells, message: AnyMessage);
  async fn system_invoke(&mut self, actor_cells: ActorCells, system_message: SystemMessage);
}

pub type AnyActorArc = Arc<Mutex<Box<dyn AnyActor>>>;

#[async_trait::async_trait]
pub trait AnyActorRef: Debug + PartialEq {
  fn path(&self) -> &ActorPath;
  async fn tell_any(&self, actor_cells: ActorCells, message: AnyMessage);
}

#[async_trait::async_trait]
pub trait SysTell: AnyActorRef {
  async fn sys_tell(&self, actor_cells: ActorCells, message: SystemMessage);
  async fn start(&mut self, actor_cells: ActorCells) {
    self.sys_tell(actor_cells, SystemMessage::Create).await;
  }
  async fn stop(&mut self, actor_cells: ActorCells) {
    self.sys_tell(actor_cells, SystemMessage::Terminate).await;
  }
  async fn suspend(&mut self, actor_cells: ActorCells) {
    self.sys_tell(actor_cells, SystemMessage::Suspend).await;
  }
  async fn resume(&mut self, actor_cells: ActorCells) {
    self.sys_tell(actor_cells, SystemMessage::Resume).await;
  }
}
