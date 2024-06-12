use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use rand::{thread_rng, RngCore};
use tokio::sync::Mutex;

use crate::core::actor::actor_context::{ActorContext, ActorContextRef};
use crate::core::actor::actor_path::ActorPath;
use crate::core::actor::actor_ref::{ActorRef, UntypedActorRef};
use crate::core::actor::{Actor, AnyActor, AnyActorArc, AnyActorRef};
use crate::core::dispatch::any_message::AnyMessage;
use crate::core::dispatch::mailbox::system_message::SystemMessage;
use crate::core::dispatch::mailbox::Mailbox;
use crate::core::dispatch::message::AutoReceivedMessage;
use crate::core::util::queue::QueueError;

#[derive(Debug)]
pub struct ActorCell<A: Actor> {
  actor: A,
  mailbox: Mailbox,
  self_ref: ActorRef<A::M>,
  parent_ref: Option<UntypedActorRef>,
  actor_context_opt: Option<ActorContextRef>,
}

impl<A: Actor> ActorCell<A> {
  pub fn new(actor: A, mailbox: Mailbox, self_ref: ActorRef<A::M>) -> Self {
    Self {
      actor,
      mailbox,
      self_ref,
      parent_ref: None,
      actor_context_opt: None,
    }
  }

  pub(crate) fn get_actor_context_ref(&self) -> ActorContextRef {
    self.actor_context_opt.as_ref().unwrap().clone()
  }

  pub(crate) async fn get_actor_context(&self) -> ActorContext {
    let actor_context_ref = self.get_actor_context_ref();
    let actor_context = actor_context_ref.upgrade().await.as_ref().unwrap().clone();
    actor_context
  }
}

#[async_trait]
impl<A: Actor + 'static> AnyActor for ActorCell<A> {
  fn path(&self) -> &ActorPath {
    &self.self_ref.path()
  }

  async fn set_parent(&mut self, parent_ref: UntypedActorRef) {
    self.parent_ref = Some(parent_ref);
  }

  fn set_actor_context_ref(&mut self, actor_context: ActorContextRef) {
    self.actor_context_opt = Some(actor_context);
  }

  async fn get_parent(&self) -> Option<UntypedActorRef> {
    self.parent_ref.clone()
  }

  async fn get_children(&self) -> Vec<AnyActorArc> {
    let actor_context = self.get_actor_context().await;
    actor_context.get_children().await
  }

  async fn send_message(&self, message: AnyMessage) -> Result<(), QueueError<AnyMessage>> {
    self.mailbox.enqueue_message(message).await
  }

  async fn send_system_message(&self, system_message: SystemMessage) -> Result<(), QueueError<SystemMessage>> {
    log::debug!("send_system_message: {:?}", system_message);
    self.mailbox.enqueue_system_message(system_message).await
  }

  async fn start(&self) -> Result<(), QueueError<SystemMessage>> {
    self.send_system_message(SystemMessage::Create).await
  }

  async fn stop(&self) -> Result<(), QueueError<SystemMessage>> {
    self.send_system_message(SystemMessage::Terminate).await
  }

  async fn suspend(&self) -> Result<(), QueueError<SystemMessage>> {
    self.send_system_message(SystemMessage::Suspend).await
  }

  async fn resume(&self) -> Result<(), QueueError<SystemMessage>> {
    self.send_system_message(SystemMessage::Resume).await
  }

  async fn child_terminated(&mut self, child: UntypedActorRef) {
    log::debug!("child_terminated: {:?}", child);
    let actor_context = self.get_actor_context().await;
    actor_context.remove_child(child.path()).await;

    if actor_context.is_child_empty().await {
      let actor_context_ref = self.get_actor_context_ref();
      let actor_context = actor_context_ref.upgrade().await.unwrap();

      self.actor.around_post_stop(actor_context.clone()).await;
      if let Some(parent_ref) = self.get_parent().await {
        parent_ref
          .tell_any(AnyMessage::new(AutoReceivedMessage::Terminated(
            self.self_ref.to_untyped(),
          )))
          .await;
      }
    }
  }

  async fn invoke(&mut self, mut message: AnyMessage) {
    if let Ok(message) = message.take::<A::M>() {
      let actor_context_ref = self.get_actor_context_ref();
      let actor_context = actor_context_ref.upgrade().await.unwrap();
      self.actor.receive(actor_context, message).await;
    }
  }

  async fn system_invoke(&mut self, system_message: SystemMessage) {
    log::debug!("system_invoke: {:?}", system_message);
    let actor_context_ref = self.get_actor_context_ref();
    let actor_context = actor_context_ref.upgrade().await.unwrap();
    match system_message {
      SystemMessage::Create => {
        log::debug!("Create: {}, suspend = {}", self.path(), self.mailbox.is_suspend().await);
        self.actor.around_pre_start(actor_context).await;
      }
      SystemMessage::Suspend => {
        log::debug!("Suspend: {}, suspend = {}", self.path(), self.mailbox.is_suspend().await);
        self.mailbox.suspend().await;
      }
      SystemMessage::Resume => {
        log::debug!("Resume: {}, suspend = {}", self.path(), self.mailbox.is_suspend().await);
        self.mailbox.resume().await;
      }
      SystemMessage::Terminate => {
        log::debug!("Terminate: {}, suspend = {}", self.path(), self.mailbox.is_suspend().await);
        self.mailbox.become_closed().await;
        if !actor_context.is_child_empty().await {
          for child in &actor_context.get_children().await {
            let child = child.lock().await;
            child.stop().await.unwrap();
          }
        } else {
          self.actor.around_post_stop(actor_context.clone()).await;
          if let Some(parent_ref) = self.get_parent().await {
            parent_ref
              .tell_any(AnyMessage::new(AutoReceivedMessage::Terminated(
                self.self_ref.to_untyped(),
              )))
              .await;
          }
        }
      }
    }
  }
}

pub const UNDEFINED_UID: u32 = 0;

pub fn new_uid() -> u32 {
  let uid = thread_rng().next_u32();
  if uid == UNDEFINED_UID {
    new_uid()
  } else {
    uid
  }
}

pub fn split_name_and_uid(name: &str) -> (&str, u32) {
  let i = name.chars().position(|c| c == '#');
  match i {
    None => (name, UNDEFINED_UID),
    Some(n) => {
      let h = &name[..n];
      let t = &name[n + 1..];
      let nn = t.parse::<u32>().unwrap();
      (h, nn)
    }
  }
}
