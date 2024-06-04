use std::fmt::{Debug, Formatter};
use std::marker::PhantomData;

use async_trait::async_trait;

use crate::core::actor::actor_cells::ActorCells;
use crate::core::actor::actor_path::ActorPath;
use crate::core::actor::actor_system::ActorSystem;
use crate::core::actor::{AnyActorRef, SysTell};
use crate::core::dispatch::any_message::AnyMessage;
use crate::core::dispatch::mailbox::system_message::SystemMessage;
use crate::core::dispatch::message::Message;

#[derive(Debug, Clone)]
pub struct UntypedActorRef {
  path: ActorPath,
}

impl UntypedActorRef {
  pub fn new(path: ActorPath) -> Self {
    Self { path }
  }
}

impl PartialEq for UntypedActorRef {
  fn eq(&self, other: &Self) -> bool {
    self.path == other.path
  }
}

#[async_trait::async_trait]
impl AnyActorRef for UntypedActorRef {
  fn path(&self) -> &ActorPath {
    &self.path
  }

  async fn tell_any(&self, actor_cells: ActorCells, message: AnyMessage) {
    if let Some(actor_arc) = actor_cells.find_actor(&self.path).await {
      log::debug!("sending a message to {}, message = {:?}", self.path, message);
      actor_arc.lock().await.send_message(message).await.unwrap();
      actor_cells.dispatch().await;
    } else {
      panic!("actor not found");
    }
  }
}

#[async_trait::async_trait]
impl SysTell for UntypedActorRef {
  async fn sys_tell(&self, actor_cells: ActorCells, message: SystemMessage) {
    if let Some(actor_arc) = actor_cells.find_actor(&self.path).await {
      actor_arc.lock().await.send_system_message(message).await.unwrap();
      actor_cells.dispatch().await;
    } else {
      panic!("actor not found");
    }
  }
}

#[derive(Debug, Clone)]
pub struct ActorRef<M: Message> {
  path: ActorPath,
  actor_cells: ActorCells,
  p: PhantomData<M>,
}

impl<M: Message> ActorRef<M> {
  pub fn new(actor_cells: ActorCells, path: ActorPath) -> Self {
    Self {
      actor_cells,
      path,
      p: PhantomData,
    }
  }

  pub fn to_untyped(&self) -> UntypedActorRef {
    UntypedActorRef::new(self.path.clone())
  }

  pub async fn tell(&self, actor_cells: ActorCells, message: M) {
    self.tell_any(actor_cells, AnyMessage::new(message)).await;
  }
}

impl<M: Message> PartialEq for ActorRef<M> {
  fn eq(&self, other: &Self) -> bool {
    self.path == other.path
  }
}

#[async_trait::async_trait]
impl<M: Message> AnyActorRef for ActorRef<M> {
  fn path(&self) -> &ActorPath {
    &self.path
  }

  async fn tell_any(&self, actor_cells: ActorCells, message: AnyMessage) {
    if let Some(actor_arc) = actor_cells.find_actor(&self.path).await {
      log::debug!("sending a message to {}, message = {:?}", self.path, message);
      actor_arc.lock().await.send_message(message).await.unwrap();
      actor_cells.dispatch().await;
    } else {
      panic!("actor not found");
    }
  }
}

#[async_trait::async_trait]
impl<M: Message> SysTell for ActorRef<M> {
  async fn sys_tell(&self, actor_cells: ActorCells, message: SystemMessage) {
    if let Some(actor_arc) = actor_cells.find_actor(&self.path).await {
      actor_arc.lock().await.send_system_message(message).await.unwrap();
      actor_cells.dispatch().await;
    } else {
      panic!("actor not found");
    }
  }
}
