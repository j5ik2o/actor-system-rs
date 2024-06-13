use std::fmt::Debug;
use std::marker::PhantomData;

use crate::core::actor::actor_context::{ActorContext, ActorContextRef};
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

impl ActorRefInner {
    fn set_actor_context_ref(&mut self, actor_context_ref: ActorContextRef) {
        self.actor_context_ref = Some(actor_context_ref);
    }

    fn path(&self) -> &ActorPath {
      &self.path
    }

    fn get_actor_context_ref(&self) -> ActorContextRef {
      self.actor_context_ref.as_ref().unwrap().clone()
    }

  async fn get_actor_context(&self) -> ActorContext {
    let actor_context_ref = self.get_actor_context_ref();
    let actor_context = actor_context_ref.upgrade().await.as_ref().unwrap().clone();
    actor_context
  }
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
    self.inner.set_actor_context_ref(actor_context_ref);
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
    let actor_context = self.inner.get_actor_context().await;
    if let Some(actor_writer_arc) = actor_context.find_actor_writer(&self.inner.path).await {
      {
        let actor_writer_mg = actor_writer_arc.lock().await;
        actor_writer_mg.send_message(message).await.unwrap();
      }
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
    let actor_context = self.inner.get_actor_context().await;
    if let Some(actor_writer_arc) = actor_context.find_actor_writer(&self.inner.path).await {
      {
        let actor_writer_arc_mg = actor_writer_arc.lock().await;
        actor_writer_arc_mg.send_system_message(message.clone()).await.unwrap();
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
    result.set_actor_context_ref(self.inner.get_actor_context_ref());
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
    let actor_context = self.inner.get_actor_context().await;
    if let Some(actor_writer_arc) = actor_context.find_actor_writer(&self.inner.path).await {
      {
        let actor_writer_arc_mg = actor_writer_arc.lock().await;
        actor_writer_arc_mg.send_message(message).await.unwrap();
      }
      actor_context.dispatch().await;
    } else {
      panic!("actor not found");
    }
  }
}

#[async_trait::async_trait]
impl<M: Message> SysTell for ActorRef<M> {
  async fn sys_tell(&self, message: SystemMessage) {
    let actor_context = self.inner.get_actor_context().await;
    if let Some(actor_writer_arc) = actor_context.find_actor_writer(&self.inner.path).await {
      {
        let actor_writer_arc_mg = actor_writer_arc.lock().await;
        actor_writer_arc_mg.send_system_message(message).await.unwrap();
      }
      actor_context.dispatch().await;
    } else {
      panic!("actor not found");
    }
  }
}
