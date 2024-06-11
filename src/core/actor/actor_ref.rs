use std::fmt::Debug;
use std::marker::PhantomData;

use crate::core::actor::actor_context::ActorContextRef;
use crate::core::actor::actor_path::ActorPath;
use crate::core::actor::{AnyActorRef, SysTell};
use crate::core::dispatch::any_message::AnyMessage;
use crate::core::dispatch::mailbox::system_message::SystemMessage;
use crate::core::dispatch::message::Message;

#[derive(Debug, Clone)]
struct ActorRefInner {
  path: ActorPath,
  actor_context_ref: ActorContextRef,
}

#[derive(Debug, Clone)]
pub struct UntypedActorRef {
  inner: ActorRefInner,
}

impl UntypedActorRef {
  pub fn new(path: ActorPath, actor_context_ref: ActorContextRef) -> Self {
    Self {
      inner: ActorRefInner {
        path,
        actor_context_ref,
      },
    }
  }
}

impl PartialEq for UntypedActorRef {
  fn eq(&self, other: &Self) -> bool {
    self.inner.path == other.inner.path
  }
}

#[async_trait::async_trait]
impl AnyActorRef for UntypedActorRef {
  fn path(&self) -> &ActorPath {
    &self.inner.path
  }

  async fn tell_any(&self, message: AnyMessage) {
    let actor_context = self.inner.actor_context_ref.upgrade().await.unwrap();
    if let Some(actor_arc) = actor_context.find_actor(&self.inner.path).await {
      log::debug!("sending a message to {}, message = {:?}", self.inner.path, message);
      actor_arc.lock().await.send_message(message).await.unwrap();
      actor_context.dispatch().await;
    } else {
      panic!("actor not found");
    }
  }
}

#[async_trait::async_trait]
impl SysTell for UntypedActorRef {
  async fn sys_tell(&self, message: SystemMessage) {
    let actor_context = self.inner.actor_context_ref.upgrade().await.unwrap();
    if let Some(actor_arc) = actor_context.find_actor(&self.inner.path).await {
      actor_arc.lock().await.send_system_message(message).await.unwrap();
      actor_context.dispatch().await;
    } else {
      panic!("actor not found");
    }
  }
}

#[derive(Debug, Clone)]
pub struct ActorRef<M: Message> {
  inner: ActorRefInner,
  p: PhantomData<M>,
}

impl<M: Message> ActorRef<M> {
  pub fn new(actor_context_ref: ActorContextRef, path: ActorPath) -> Self {
    Self {
      inner: ActorRefInner {
        actor_context_ref,
        path,
      },
      p: PhantomData,
    }
  }

  pub fn to_untyped(&self) -> UntypedActorRef {
    UntypedActorRef::new(self.inner.path.clone(), self.inner.actor_context_ref.clone())
  }

  pub async fn tell(&self, message: M) {
    self.tell_any(AnyMessage::new(message)).await;
  }
}

impl<M: Message> PartialEq for ActorRef<M> {
  fn eq(&self, other: &Self) -> bool {
    self.inner.path == other.inner.path
  }
}

#[async_trait::async_trait]
impl<M: Message> AnyActorRef for ActorRef<M> {
  fn path(&self) -> &ActorPath {
    &self.inner.path
  }

  async fn tell_any(&self, message: AnyMessage) {
    let actor_context = self.inner.actor_context_ref.upgrade().await.unwrap();
    if let Some(actor_arc) = actor_context.find_actor(&self.inner.path).await {
      log::debug!("sending a message to {}, message = {:?}", self.inner.path, message);
      actor_arc.lock().await.send_message(message).await.unwrap();
      actor_context.dispatch().await;
    } else {
      panic!("actor not found");
    }
  }
}

#[async_trait::async_trait]
impl<M: Message> SysTell for ActorRef<M> {
  async fn sys_tell(&self, message: SystemMessage) {
    let actor_context = self.inner.actor_context_ref.upgrade().await.unwrap();
    if let Some(actor_arc) = actor_context.find_actor(&self.inner.path).await {
      actor_arc.lock().await.send_system_message(message).await.unwrap();
      actor_context.dispatch().await;
    } else {
      panic!("actor not found");
    }
  }
}
