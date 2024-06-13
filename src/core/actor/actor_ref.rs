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
  actor_context_ref: Option<ActorContextRef>,
}

#[derive(Debug, Clone)]
pub struct UntypedActorRef {
  inner: ActorRefInner,
}

impl UntypedActorRef {
  pub fn new(path: ActorPath) -> Self {
    Self {
      inner: ActorRefInner {
        path,
        actor_context_ref: None,
      },
    }
  }
  pub fn set_actor_context_ref(&mut self, actor_context_ref: ActorContextRef) {
    self.inner.actor_context_ref = Some(actor_context_ref);
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
    let actor_context = self.inner.actor_context_ref.as_ref().unwrap().upgrade().await.unwrap();
    if let Some(actor_arc) = actor_context.find_actor_writer(&self.inner.path).await {
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
    log::debug!("sys_tell: {:?}", message);
    let actor_context = self.inner.actor_context_ref.as_ref().unwrap().upgrade().await.unwrap();
    if let Some(actor_arc) = actor_context.find_actor_writer(&self.inner.path).await {
      {
        let actor_mg = actor_arc.lock().await;
        actor_mg.send_system_message(message.clone()).await.unwrap();
      }
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
        actor_context_ref: Some(actor_context_ref),
        path,
      },
      p: PhantomData,
    }
  }

  pub fn to_untyped(&self) -> UntypedActorRef {
    let mut result = UntypedActorRef::new(self.inner.path.clone());
    result.set_actor_context_ref(self.inner.actor_context_ref.as_ref().unwrap().clone());
    result
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
    let actor_context = self.inner.actor_context_ref.as_ref().unwrap().upgrade().await.unwrap();
    if let Some(actor_arc) = actor_context.find_actor_writer(&self.inner.path).await {
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
    let actor_context = self.inner.actor_context_ref.as_ref().unwrap().upgrade().await.unwrap();
    if let Some(actor_arc) = actor_context.find_actor_writer(&self.inner.path).await {
      let lock = actor_arc.lock().await;
      lock.send_system_message(message).await.unwrap();
      actor_context.dispatch().await;
    } else {
      panic!("actor not found");
    }
  }
}
