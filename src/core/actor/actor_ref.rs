use std::marker::PhantomData;

use async_trait::async_trait;

use crate::core::actor::actor_path::ActorPath;
use crate::core::actor::actor_system::ActorSystem;
use crate::core::dispatch::any_message::AnyMessage;
use crate::core::dispatch::message::Message;

#[derive(Debug, Clone)]
pub struct ActorRef<M: Message> {
  path: ActorPath,
  p: PhantomData<M>,
}

impl<M: Message> ActorRef<M> {
  pub fn new(path: ActorPath) -> Self {
    Self { path, p: PhantomData }
  }

  pub async fn tell(&self, system: &ActorSystem, message: M) {
    self.tell_any(system, AnyMessage::new(message)).await;
  }
}

#[async_trait::async_trait]
pub trait AnyActorRef {
  fn path(&self) -> &ActorPath;
  async fn tell_any(&self, system: &ActorSystem, message: AnyMessage);
}

#[async_trait::async_trait]
impl<M: Message> AnyActorRef for ActorRef<M> {
  fn path(&self) -> &ActorPath {
    &self.path
  }

  async fn tell_any(&self, system: &ActorSystem, message: AnyMessage) {
    if let Some(actor_arc) = system.find_actor(&self.path).await {
      log::debug!("sending a message to {}, message = {:?}", self.path, message);
      actor_arc.lock().await.send_message(message).await.unwrap();
      system.dispatch().await;
    } else {
      panic!("actor not found");
    }
  }
}
