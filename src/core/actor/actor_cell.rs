use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use rand::{thread_rng, RngCore};
use tokio::sync::Mutex;

use crate::core::actor::actor_path::ActorPath;
use crate::core::actor::actor_ref::{ActorRef, UntypedActorRef};
use crate::core::actor::actor_system::ActorSystem;
use crate::core::actor::{Actor, AnyActor, AnyActorArc, AnyActorRef};
use crate::core::actor::actor_cells::ActorCells;
use crate::core::dispatch::any_message::AnyMessage;
use crate::core::dispatch::mailbox::system_message::SystemMessage;
use crate::core::dispatch::mailbox::Mailbox;
use crate::core::dispatch::message::AutoReceivedMessage;
use crate::core::util::queue::{QueueError, QueueWriteBehavior, QueueWriter};

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
#[derive(Debug)]
pub struct ActorCell<A: Actor> {
  actor: A,
  mailbox: Mailbox,
  self_ref: ActorRef<A::M>,
  parent_ref: Option<UntypedActorRef>,
  children: Arc<Mutex<HashMap<ActorPath, AnyActorArc>>>,
}

impl<A: Actor> ActorCell<A> {
  pub fn new(actor: A, mailbox: Mailbox, self_ref: ActorRef<A::M>) -> Self {
    Self {
      actor,
      mailbox,
      self_ref,
      parent_ref: None,
      children: Arc::new(Mutex::new(HashMap::new())),
    }
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

  async fn get_parent(&self) -> Option<UntypedActorRef> {
    self.parent_ref.clone()
  }

  async fn add_child(&self, child_cell: AnyActorArc) {
    let mut children = self.children.lock().await;
    let child_cell_lock = child_cell.lock().await;
    let path = child_cell_lock.path();
    children.insert(path.clone(), child_cell.clone());
  }

  async fn get_children(&self) -> Vec<AnyActorArc> {
    let children = self.children.lock().await;
    children.values().cloned().collect()
  }

  async fn send_message(&self, message: AnyMessage) -> Result<(), QueueError<AnyMessage>> {
    self.mailbox.enqueue_message(message).await
  }

  async fn send_system_message(&self, system_message: SystemMessage) -> Result<(), QueueError<SystemMessage>> {
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

  async fn child_terminated(&mut self, actor_cells: ActorCells, child: UntypedActorRef) {
    log::debug!("child_terminated: {:?}", child);
    let mut children_lock = self.children.lock().await;
    children_lock.remove(child.path());

    if children_lock.is_empty() {
      self.actor.around_post_stop(actor_cells.clone()).await;
      if let Some(parent_ref) = self.get_parent().await {
        parent_ref
          .tell_any(
            actor_cells,
            AnyMessage::new(AutoReceivedMessage::Terminated(self.self_ref.to_untyped())),
          )
          .await;
      }
    }
  }

  async fn invoke(&mut self, actor_cells: ActorCells, mut message: AnyMessage) {
    if let Ok(message) = message.take::<A::M>() {
      self.actor.receive(actor_cells, message).await;
    }
  }

  async fn system_invoke(&mut self, actor_cells: ActorCells, system_message: SystemMessage) {
    log::debug!("system_invoke: {:?}", system_message);
    match system_message {
      SystemMessage::Create => {
        log::debug!("Create: {}", self.path());
        self.actor.around_pre_start(actor_cells).await;
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
        self.mailbox.become_closed().await;
        let children_lock = self.children.lock().await;
        if !children_lock.is_empty() {
          for (_, child) in children_lock.iter() {
            let mut child = child.lock().await;
            child.stop().await.unwrap();
          }
        } else {
          self.actor.around_post_stop(actor_cells.clone()).await;
          if let Some(parent_ref) = self.get_parent().await {
            parent_ref
              .tell_any(
                actor_cells,
                AnyMessage::new(AutoReceivedMessage::Terminated(self.self_ref.to_untyped())),
              )
              .await;
          }
        }
      }
    }
  }
}
