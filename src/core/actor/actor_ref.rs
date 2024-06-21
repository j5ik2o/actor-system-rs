use std::fmt::{Debug, Formatter};
use std::hash::{Hash, Hasher};
use std::marker::PhantomData;

use crate::core::actor::actor_context::{ActorContext, ActorContextRef};
use crate::core::actor::actor_path::ActorPath;
use crate::core::actor::{AnyActorRef, AnyActorWriterArc, SysTell};
use crate::core::dispatch::any_message::AnyMessage;
use crate::core::dispatch::mailbox::system_message::SystemMessage;
use crate::core::dispatch::message::Message;

#[derive(Debug, Clone)]
pub enum InternalActorRef {
  Local(LocalActorRef),
  Ignore,
}

impl InternalActorRef {
  pub fn new_local(actor_ref: LocalActorRef) -> Self {
    InternalActorRef::Local(actor_ref)
  }

  pub fn new_ignore() -> Self {
    InternalActorRef::Ignore
  }

  pub fn to_typed<M: Message>(&self) -> TypedActorRef<M> {
    match self {
      InternalActorRef::Local(ref actor_ref) => {
        let actor_context_ref = actor_ref.inner.get_actor_context_ref();
        let mut result = TypedActorRef::new(actor_context_ref, actor_ref.inner.path.clone());
        result.set_actor_cell_writer(actor_ref.inner.get_actor_cell_writer());
        result
      }
      InternalActorRef::Ignore => panic!("InternalActorRef::to_typed: not supported"),
    }
  }
}

impl std::fmt::Display for InternalActorRef {
  fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
    match self {
      InternalActorRef::Local(ref actor_ref) => write!(f, "{}", actor_ref),
      InternalActorRef::Ignore => write!(f, "Ignore"),
    }
  }
}

impl Hash for InternalActorRef {
  fn hash<H: Hasher>(&self, state: &mut H) {
    match self {
      InternalActorRef::Local(ref actor_ref) => actor_ref.hash(state),
      InternalActorRef::Ignore => "Ignore".hash(state),
    }
  }
}

#[async_trait::async_trait]
impl AnyActorRef for InternalActorRef {
  fn path(&self) -> &ActorPath {
    match self {
      InternalActorRef::Local(ref actor_ref) => actor_ref.path(),
      InternalActorRef::Ignore => panic!("InternalActorRef::path: not supported"),
    }
  }

  async fn tell_any(&self, message: AnyMessage) {
    match self {
      InternalActorRef::Local(ref actor_ref) => actor_ref.tell_any(message).await,
      InternalActorRef::Ignore => {}
    }
  }
}

impl Eq for InternalActorRef {}
impl PartialEq for InternalActorRef {
  fn eq(&self, other: &Self) -> bool {
    match (self, other) {
      (InternalActorRef::Local(ref a), InternalActorRef::Local(ref b)) => a == b,
      (InternalActorRef::Ignore, InternalActorRef::Ignore) => true,
      _ => false,
    }
  }
}

#[async_trait::async_trait]
impl SysTell for InternalActorRef {
  async fn sys_tell(&self, message: SystemMessage) {
    match self {
      InternalActorRef::Local(ref actor_ref) => actor_ref.sys_tell(message).await,
      _ => {}
    }
  }

  async fn when_terminated(&self) {
    match self {
      InternalActorRef::Local(ref actor_ref) => actor_ref.when_terminated().await,
      _ => {}
    }
  }
}

impl InternalActorRef {
  pub fn is_local(&self) -> bool {
    match self {
      InternalActorRef::Local(_) => true,
      _ => false,
    }
  }
}

#[derive(Debug, Clone)]
struct ActorRefInner {
  path: ActorPath,
  actor_cell_writer: Option<AnyActorWriterArc>,
  actor_context_ref: Option<ActorContextRef>,
}

impl Eq for ActorRefInner {}

impl PartialEq for ActorRefInner {
  fn eq(&self, other: &Self) -> bool {
    self.path == other.path
  }
}

impl ActorRefInner {
  fn set_actor_context_ref(&mut self, actor_context_ref: ActorContextRef) {
    self.actor_context_ref = Some(actor_context_ref);
  }

  fn path(&self) -> &ActorPath {
    &self.path
  }

  fn get_actor_cell_writer(&self) -> AnyActorWriterArc {
    self.actor_cell_writer.as_ref().unwrap().clone()
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

#[derive(Debug, Clone, Eq)]
pub struct LocalActorRef {
  inner: ActorRefInner,
}

impl LocalActorRef {
  pub fn new(path: ActorPath) -> Self {
    Self {
      inner: ActorRefInner {
        path,
        actor_cell_writer: None,
        actor_context_ref: None,
      },
    }
  }

  pub fn set_actor_context_ref(&mut self, actor_context_ref: ActorContextRef) {
    self.inner.set_actor_context_ref(actor_context_ref);
  }

  pub fn set_actor_cell_writer(&mut self, cell_writer: AnyActorWriterArc) {
    self.inner.actor_cell_writer = Some(cell_writer);
  }
}

impl std::fmt::Display for LocalActorRef {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    write!(f, "{}", self.inner.path)
  }
}

impl Hash for LocalActorRef {
  fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
    self.inner.path.hash(state);
  }
}

impl PartialEq for LocalActorRef {
  fn eq(&self, other: &Self) -> bool {
    self.inner.path == other.inner.path
  }
}

#[async_trait::async_trait]
impl AnyActorRef for LocalActorRef {
  fn path(&self) -> &ActorPath {
    &self.inner.path
  }

  async fn tell_any(&self, message: AnyMessage) {
    let actor_context = self.inner.get_actor_context().await;
    {
      let actor_cell_writer = self.inner.get_actor_cell_writer();
      let actor_writer_mg = actor_cell_writer.lock().await;
      actor_writer_mg.send_message(message).await.unwrap();
    }
    actor_context.dispatch().await;
  }
}

#[async_trait::async_trait]
impl SysTell for LocalActorRef {
  async fn sys_tell(&self, message: SystemMessage) {
    log::debug!("sys_tell: {:?}", message);
    let actor_context = self.inner.get_actor_context().await;
    {
      let actor_cell_writer = self.inner.get_actor_cell_writer();
      let actor_writer_arc_mg = actor_cell_writer.lock().await;
      actor_writer_arc_mg.send_system_message(message.clone()).await.unwrap();
    }
    actor_context.dispatch().await;
  }

  async fn when_terminated(&self) {
    let terminate_notify = {
      let actor_cell_writer = self.inner.get_actor_cell_writer();
      let actor_writer_arc_mg = actor_cell_writer.lock().await;
      actor_writer_arc_mg.get_terminate_notify().await.clone()
    };
    log::debug!("when_terminated: start {}", self.inner.path());
    terminate_notify.notified().await;
    log::debug!("when_terminated: finish {}", self.inner.path());
  }
}

#[derive(Debug, Clone)]
pub struct TypedActorRef<M: Message> {
  inner: ActorRefInner,
  p: PhantomData<M>,
}

impl<M: Message> TypedActorRef<M> {
  pub fn new(actor_context_ref: ActorContextRef, path: ActorPath) -> Self {
    Self {
      inner: ActorRefInner {
        path,
        actor_cell_writer: None,
        actor_context_ref: Some(actor_context_ref),
      },
      p: PhantomData,
    }
  }

  pub(crate) async fn get_actor_context(&self) -> ActorContext {
    self.inner.get_actor_context().await
  }

  pub fn set_actor_cell_writer(&mut self, cell_writer: AnyActorWriterArc) {
    self.inner.actor_cell_writer = Some(cell_writer);
  }

  pub fn to_untyped(&self) -> InternalActorRef {
    let mut result = LocalActorRef::new(self.inner.path.clone());
    result.set_actor_context_ref(self.inner.get_actor_context_ref());
    result.set_actor_cell_writer(self.inner.get_actor_cell_writer());
    InternalActorRef::Local(result)
  }

  pub async fn tell(&self, message: M) {
    self.tell_any(AnyMessage::new(message)).await;
  }
}

impl<M: Message> std::fmt::Display for TypedActorRef<M> {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    write!(f, "{}", self.inner.path)
  }
}

impl<M: Message> PartialEq for TypedActorRef<M> {
  fn eq(&self, other: &Self) -> bool {
    self.inner.path == other.inner.path
  }
}

#[async_trait::async_trait]
impl<M: Message> AnyActorRef for TypedActorRef<M> {
  fn path(&self) -> &ActorPath {
    &self.inner.path
  }

  async fn tell_any(&self, message: AnyMessage) {
    let actor_context = self.inner.get_actor_context().await;
    {
      let actor_cell_writer = self.inner.get_actor_cell_writer();
      let actor_writer_arc_mg = actor_cell_writer.lock().await;
      actor_writer_arc_mg.send_message(message).await.unwrap();
    }
    actor_context.dispatch().await;
  }
}

#[async_trait::async_trait]
impl<M: Message> SysTell for TypedActorRef<M> {
  async fn sys_tell(&self, message: SystemMessage) {
    let actor_context = self.inner.get_actor_context().await;
    {
      let actor_cell_writer = self.inner.get_actor_cell_writer();
      let actor_writer_arc_mg = actor_cell_writer.lock().await;
      actor_writer_arc_mg.send_system_message(message).await.unwrap();
    }
    actor_context.dispatch().await;
  }

  async fn when_terminated(&self) {
    let terminate_notify = {
      let actor_cell_writer = self.inner.get_actor_cell_writer();
      let actor_writer_arc_mg = actor_cell_writer.lock().await;
      actor_writer_arc_mg.get_terminate_notify().await.clone()
    };
    log::debug!("when_terminated: start {}", self.inner.path());
    terminate_notify.notified().await;
    log::debug!("when_terminated: finish {}", self.inner.path());
  }
}
