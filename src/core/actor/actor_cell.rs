use std::sync::Arc;

use async_trait::async_trait;

use crate::core::actor::actor_context::ActorContext;
use crate::core::actor::actor_ref::ActorRef;
use crate::core::actor::actor_system::ActorSystem;
use crate::core::actor::{Actor, AnyActor};
use crate::core::dispatch::any_message::AnyMessage;
use crate::core::dispatch::mailbox::system_message::SystemMessage;
use crate::core::dispatch::mailbox::Mailbox;
use crate::core::util::queue::{QueueError, QueueWriteBehavior, QueueWriter};

#[derive(Debug)]
pub struct ActorCell<A: Actor> {
  actor: A,
  mailbox: Mailbox,
  self_ref: ActorRef<A::M>,
  system: Arc<ActorSystem>,
}

impl<A: Actor> ActorCell<A> {
  pub fn new(actor: A, mailbox: Mailbox, self_ref: ActorRef<A::M>, system: Arc<ActorSystem>) -> Self {
    Self {
      actor,
      mailbox,
      self_ref,
      system,
    }
  }
}

#[async_trait]
impl<A: Actor + 'static> AnyActor for ActorCell<A> {
  fn path(&self) -> String {
    self.self_ref.path().to_string()
  }

  async fn send_message(&mut self, message: AnyMessage) -> Result<(), QueueError<AnyMessage>> {
    self.mailbox.send_message(message).await
  }

  async fn send_system_message(&mut self, system_message: SystemMessage) -> Result<(), QueueError<SystemMessage>> {
    self.mailbox.send_system_message(system_message).await
  }

  async fn start(&mut self) -> Result<(), QueueError<SystemMessage>> {
    self.send_system_message(SystemMessage::Create).await
  }

  async fn stop(&mut self) -> Result<(), QueueError<SystemMessage>> {
    self.send_system_message(SystemMessage::Terminate).await
  }

  async fn suspend(&mut self) -> Result<(), QueueError<SystemMessage>> {
    self.send_system_message(SystemMessage::Suspend).await
  }

  async fn resume(&mut self) -> Result<(), QueueError<SystemMessage>> {
    self.send_system_message(SystemMessage::Resume).await
  }

  async fn invoke(&mut self, mut message: AnyMessage) {
    if let Ok(message) = message.take::<A::M>() {
      let ctx = ActorContext::new(self.self_ref.clone(), self.system.clone());
      self.actor.receive(ctx, message).await;
    }
  }

  async fn system_invoke(&mut self, system_message: SystemMessage) {
    log::debug!("system_invoke: {:?}", system_message);
    match system_message {
      SystemMessage::Create => {
        log::debug!("Create: {}", self.path());
        let ctx = ActorContext::new(self.self_ref.clone(), self.system.clone());
        self.actor.around_pre_start(ctx).await;
      }
      SystemMessage::Suspend => {
        log::debug!("Suspend: {}", self.path());
        self.mailbox.suspend().await;
      }
      SystemMessage::Resume => {
        log::debug!("Resume: {}", self.path());
        self.mailbox.resume().await;
      }
      SystemMessage::Terminate => {
        log::debug!("Terminate: {}", self.path());
        let ctx = ActorContext::new(self.self_ref.clone(), self.system.clone());
        self.mailbox.become_closed().await;
        self.actor.around_post_stop(ctx).await;
      }
    }
  }
}
