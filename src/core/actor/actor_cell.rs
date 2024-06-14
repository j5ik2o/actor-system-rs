use std::collections::{HashMap, HashSet};
use std::error::Error;
use std::sync::Arc;

use async_trait::async_trait;
use rand::{thread_rng, RngCore};
use tokio::sync::{Mutex, Notify};
use tokio_condvar::Condvar;

use crate::core::actor::actor_context::{ActorContext, ActorContextRef};
use crate::core::actor::actor_path::{ActorPath, ActorPathBehavior};
use crate::core::actor::actor_ref::{ActorRef, UntypedActorRef};
use crate::core::actor::supervisor_strategy::SupervisorStrategy;
use crate::core::actor::{Actor, AnyActorReader, AnyActorRef, AnyActorWriter, AnyActorWriterArc, SysTell};
use crate::core::dispatch::any_message::AnyMessage;
use crate::core::dispatch::mailbox::system_message::SystemMessage;
use crate::core::dispatch::mailbox::Mailbox;
use crate::core::dispatch::message::AutoReceivedMessage;
use crate::core::util::queue::QueueError;

pub struct DeathWatch {
  actor_context_ref: ActorContextRef,
  watching: HashMap<UntypedActorRef, Option<AnyMessage>>,
  watched_by: HashSet<UntypedActorRef>,
}

impl DeathWatch {
  pub fn new(actor_context_ref: ActorContextRef) -> Self {
    Self {
      actor_context_ref,
      watching: HashMap::new(),
      watched_by: HashSet::new(),
    }
  }

  fn is_watching(&self, subject: &UntypedActorRef) -> bool {
    self.watching.contains_key(subject)
  }

  async fn update_watching(&mut self, subject: &UntypedActorRef, message: Option<AnyMessage>) {
    if let Some(v) = self.watching.get_mut(subject) {
      *v = message;
    }
  }

  async fn check_watching_some(&self, subject: &UntypedActorRef) {
    let previous = self.watching.get(&subject);
    if previous != None {
      panic!("Watched by: {}", subject.path());
    }
  }

  pub async fn watch(&mut self, subject: UntypedActorRef) -> UntypedActorRef {
    let self_ref = self.actor_context_ref.upgrade().await.unwrap().self_ref().await;
    if subject != self_ref {
      if !self.watching.contains_key(&subject) {
        subject
          .sys_tell(SystemMessage::Watch {
            watchee: self_ref.clone(),
            watcher: self_ref.clone(),
          })
          .await;
        self.update_watching(&subject, None).await;
      } else {
        self.check_watching_some(&subject).await;
      }
    }
    subject
  }

  pub async fn add_watcher(&mut self, watchee: UntypedActorRef, watcher: UntypedActorRef) {
    let self_ref = self.actor_context_ref.upgrade().await.unwrap().self_ref().await;
    let watchee_self = watchee == self_ref;
    let watcher_self = watcher == self_ref;

    if watchee_self && !watcher_self {
      if self.watched_by.iter().any(|w| *w == watcher) {
        self.watched_by.insert(watcher);
      }
    } else if !watchee_self && watcher_self {
      self.watch(watchee).await;
    } else {
      panic!(
        "Invalid watch: watchee = {}, watcher = {}",
        watchee.path(),
        watcher.path()
      );
    }
  }
}

#[derive(Debug, Clone)]
pub struct ActorCellWriter {
  mailbox: Mailbox,
  actor_context_ref_opt: Option<ActorContextRef>,
  terminate_notify: Arc<Notify>,
}

#[derive(Debug)]
pub struct ActorCellReader<A: Actor> {
  actor: A,
  mailbox: Mailbox,
  actor_context_ref_opt: Option<ActorContextRef>,
  terminate_notify: Arc<Notify>,
}

impl ActorCellWriter {
  pub fn new(mailbox: Mailbox, terminate_notify: Arc<Notify>) -> Self {
    Self {
      mailbox,
      actor_context_ref_opt: None,
      terminate_notify,
    }
  }

  pub(crate) fn get_actor_context_ref(&self) -> ActorContextRef {
    self.actor_context_ref_opt.as_ref().unwrap().clone()
  }

  pub(crate) async fn get_actor_context(&self) -> ActorContext {
    let actor_context_ref = self.get_actor_context_ref();
    let actor_context = actor_context_ref.upgrade().await.as_ref().unwrap().clone();
    actor_context
  }
}

impl<A: Actor + 'static> ActorCellReader<A> {
  pub fn new(actor: A, mailbox: Mailbox, terminate_notify: Arc<Notify>) -> Self {
    Self {
      actor,
      mailbox,
      actor_context_ref_opt: None,
      terminate_notify,
    }
  }

  pub(crate) fn get_actor_context_ref(&self) -> ActorContextRef {
    self.actor_context_ref_opt.as_ref().unwrap().clone()
  }

  pub(crate) async fn get_actor_context(&self) -> ActorContext {
    let actor_context_ref = self.get_actor_context_ref();
    let actor_context = actor_context_ref.upgrade().await.as_ref().unwrap().clone();
    actor_context
  }

  async fn handle_invoke_failure(&mut self, cause: Arc<Box<dyn Error + Send>>) {
    log::error!("handle_invoke_failure: {:?}", cause);
    let actor_context = self.get_actor_context().await;
    let mut self_ref = actor_context.self_ref().await;
    self_ref.suspend().await;
    let child_refs = actor_context.get_child_refs().await;
    for mut child_ref in child_refs {
      child_ref.suspend().await;
    }
    let mut parent_ref_opt = match self.get_parent().await {
      Some(parent_ref) if parent_ref.path().is_child() => Some(parent_ref),
      _ => None,
    };
    if let Some(parent_ref) = &mut parent_ref_opt {
      parent_ref
        .sys_tell(SystemMessage::Failed {
          child_ref: self_ref.clone(),
          cause,
        })
        .await;
    }
  }
}

#[async_trait]
impl AnyActorWriter for ActorCellWriter {
  async fn path(&self) -> ActorPath {
    self.get_actor_context().await.self_ref().await.path().clone()
  }

  fn set_actor_context_ref(&mut self, actor_context_ref: ActorContextRef) {
    self.actor_context_ref_opt = Some(actor_context_ref);
  }

  async fn get_parent(&self) -> Option<UntypedActorRef> {
    let result = self.get_actor_context().await.get_parent_context().await;
    match result {
      Some(parent_context) => Some(parent_context.self_ref().await.clone()),
      None => None,
    }
  }

  async fn get_children(&self) -> Vec<AnyActorWriterArc> {
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

  async fn get_terminate_notify(&self) -> Arc<Notify> {
    self.terminate_notify.clone()
  }
}

#[async_trait]
impl<A: Actor + 'static> AnyActorReader for ActorCellReader<A> {
  async fn path(&self) -> ActorPath {
    self.get_actor_context().await.self_ref().await.path().clone()
  }

  async fn get_parent(&self) -> Option<UntypedActorRef> {
    let result = self.get_actor_context().await.get_parent_context().await;
    match result {
      Some(parent_context) => Some(parent_context.self_ref().await.clone()),
      None => None,
    }
  }

  fn set_actor_context_ref(&mut self, actor_context_ref: ActorContextRef) {
    self.actor_context_ref_opt = Some(actor_context_ref);
  }

  async fn child_terminated(&mut self, child: UntypedActorRef) {
    log::debug!("child_terminated: {}", child.path());
    let actor_context = self.get_actor_context().await;
    actor_context.remove_child(child.path()).await;
    self.actor.child_terminated(actor_context.clone(), child).await;
    if actor_context.is_child_empty().await {
      self.actor.all_children_terminated(actor_context.clone()).await;
    }
  }

  async fn invoke(&mut self, mut message: AnyMessage) {
    if let Ok(message) = message.take::<A::M>() {
      let actor_context = self.get_actor_context().await;
      let result = self.actor.receive(actor_context, message).await;
      if let Err(error) = result {
        self.handle_invoke_failure(Arc::new(error)).await;
      }
    }
  }

  async fn system_invoke(&mut self, system_message: SystemMessage) {
    log::debug!("system_invoke: {:?}", system_message);
    let actor_context = self.get_actor_context().await;
    match system_message {
      SystemMessage::Create => {
        log::debug!(
          "Create: {}, suspend = {}",
          self.path().await,
          self.mailbox.is_suspend().await
        );
        self.actor.around_pre_start(actor_context).await;
      }
      SystemMessage::Suspend => {
        log::debug!(
          "Suspend: {}, suspend = {}",
          self.path().await,
          self.mailbox.is_suspend().await
        );
        self.mailbox.suspend().await;
      }
      SystemMessage::Resume => {
        log::debug!(
          "Resume: {}, suspend = {}",
          self.path().await,
          self.mailbox.is_suspend().await
        );
        self.mailbox.resume().await;
      }
      SystemMessage::Watch { watchee, watcher } => {
        log::debug!("Watch: {}", self.path().await);
      }
      SystemMessage::Unwatch { watchee, watcher } => {
        log::debug!("Unwatch: {}", self.path().await);
      }
      SystemMessage::Failed { child_ref, cause } => {
        log::debug!("Failed: {}", self.path().await);
        // TODO: 親アクターで失敗を処理する
        // if !self.actor.supervisorStrategy().handleFailure(ctx,child_ref,cause) {
        //  self.handle_invoke_failure(cause).await;
        // }
      }
      SystemMessage::Recreate { cause } => {
        log::debug!("Recreate: {}", self.path().await);
      }
      SystemMessage::Terminate => {
        log::debug!(
          "Terminate: {}, suspend = {}",
          self.path().await,
          self.mailbox.is_suspend().await
        );
        if !actor_context.is_child_empty().await {
          for mut child in actor_context.get_child_refs().await {
            child.stop().await;
          }
        } else {
          let parent_ref_opt = match self.get_parent().await {
            Some(parent_ref) if parent_ref.path().is_child() => Some(parent_ref),
            _ => None,
          };
          if let Some(parent_ref) = &parent_ref_opt {
            let self_ref = self.get_actor_context().await.self_ref().await;
            parent_ref
              .tell_any(AnyMessage::new(AutoReceivedMessage::Terminated(self_ref)))
              .await;
          }
          self.actor.around_post_stop(actor_context.clone()).await;
          self.mailbox.become_closed().await;
          if parent_ref_opt.is_none() {
            self.terminate_notify.notify_waiters();
          }
        }
      }
    }
  }

  async fn get_terminate_notify(&self) -> Arc<Notify> {
    self.terminate_notify.clone()
  }

  fn supervisor_strategy(&self) -> Arc<Box<dyn SupervisorStrategy>> {
    todo!()
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
