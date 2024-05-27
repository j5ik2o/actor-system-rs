use std::marker::PhantomData;

use crate::core::actor::actor_system::ActorSystem;
use crate::core::dispatch::any_message::AnyMessage;
use crate::core::dispatch::message::Message;
use crate::ActorPath;

#[derive(Debug, Clone)]
pub struct ActorRef<M: Message> {
  path: ActorPath,
  p: PhantomData<M>,
}

impl<M: Message> ActorRef<M> {
  pub fn new(path: ActorPath) -> Self {
    Self { path, p: PhantomData }
  }

  pub fn path(&self) -> &ActorPath {
    &self.path
  }

  pub async fn tell(&self, system: &ActorSystem, message: M) {
    if let Some(actor) = system.actors.lock().await.get(&self.path) {
      log::debug!("sending a message to {}, message = {:?}", self.path, message);
      let any_message = AnyMessage::new(message);
      actor.lock().await.send_message(any_message).await.unwrap();
      system.dispatch().await;
    } else {
      panic!("actor not found");
    }
  }
}
