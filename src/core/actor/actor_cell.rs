use std::sync::Arc;

use async_trait::async_trait;
use rand::{thread_rng, RngCore};
use tokio::sync::{Notify};

use crate::core::actor::actor_context::{ActorContext, ActorContextRef};
use crate::core::actor::actor_path::{ActorPath, ActorPathBehavior};
use crate::core::actor::actor_ref::{UntypedActorRef};
use crate::core::actor::supervisor_strategy::SupervisorStrategy;
use crate::core::actor::{Actor, ActorError, AnyActorReader, AnyActorRef, AnyActorWriter, AnyActorWriterArc, SysTell};
use crate::core::dispatch::any_message::AnyMessage;
use crate::core::dispatch::mailbox::system_message::SystemMessage;
use crate::core::dispatch::mailbox::Mailbox;
use crate::core::dispatch::message::AutoReceivedMessage;
use crate::core::util::queue::QueueError;

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

  async fn handle_invoke_failure(&mut self, cause: Arc<ActorError>) {
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

  async fn handle_create(&mut self) {
    let actor_context = self.get_actor_context().await;
    log::debug!(
      "Create: path = {}, suspend = {}",
      self.path().await,
      self.mailbox.is_suspend().await
    );
    self.actor.around_pre_start(actor_context).await;
  }

  async fn handle_suspend(&mut self) {
    log::debug!(
      "Suspend: path = {}, suspend = {}",
      self.path().await,
      self.mailbox.is_suspend().await
    );
    self.mailbox.suspend().await;
  }

  async fn handle_resume(&mut self, cause: Arc<ActorError>) {
    log::debug!(
      "Resume: case = {}, path = {}, suspend = {}",
      cause,
      self.path().await,
      self.mailbox.is_suspend().await
    );
    self.mailbox.resume().await;
  }

  async fn handle_watch(&mut self, watchee: UntypedActorRef, watcher: UntypedActorRef) {
    log::debug!(
      "Watch: watchee = {}, watcher {}, path = {}, suspend = {}",
      watchee,
      watcher,
      self.path().await,
      self.mailbox.is_suspend().await
    );
  }

  async fn handle_unwatch(&mut self, watchee: UntypedActorRef, watcher: UntypedActorRef) {
    log::debug!(
      "Unwatch: watchee = {}, watcher {}, path = {}, suspend = {}",
      watchee,
      watcher,
      self.path().await,
      self.mailbox.is_suspend().await
    );
  }

  async fn handle_failed(&mut self, child_ref: &mut UntypedActorRef, cause: Arc<ActorError>) {
    let actor_context = self.get_actor_context().await;
    log::debug!("Failed: {}", self.path().await);
    if !self
      .actor
      .supervisor_strategy()
      .await
      .handle_failure(actor_context, false, child_ref.clone(), cause.clone(), vec![])
      .await
    {
      self.handle_invoke_failure(cause).await;
    }
  }

  async fn handle_recreate(&mut self, cause: Arc<ActorError>) {
    log::debug!(
      "Recreate: cause = {}, path = {}, suspend = {}",
      cause,
      self.path().await,
      self.mailbox.is_suspend().await
    );
  }

  async fn handle_terminated(&mut self) {
    let actor_context = self.get_actor_context().await;
    log::debug!(
      "Terminate: path = {}, suspend = {}",
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

  async fn handle_supervise(&mut self, child: UntypedActorRef, r#async: bool) {
    log::debug!(
      "Supervise: child = {}, async = {}, path = {}, suspend = {}",
      child,
      r#async,
      self.path().await,
      self.mailbox.is_suspend().await
    );
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

  async fn resume(&self, cause: Arc<ActorError>) -> Result<(), QueueError<SystemMessage>> {
    self
      .send_system_message(SystemMessage::Resume {
        caused_by_failure: cause,
      })
      .await
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

  async fn system_invoke(&mut self, mut system_message: SystemMessage) {
    log::debug!("system_invoke: {:?}", system_message);
    match system_message {
      SystemMessage::Create => self.handle_create().await,
      SystemMessage::Recreate { cause } => self.handle_recreate(cause).await,
      SystemMessage::Suspend => self.handle_suspend().await,
      SystemMessage::Resume {
        caused_by_failure: cause,
      } => self.handle_resume(cause).await,
      SystemMessage::Terminate => self.handle_terminated().await,
      SystemMessage::Supervise { .. } => {} // use RepointableActorRef
      SystemMessage::Watch { watchee, watcher } => self.handle_watch(watchee, watcher).await,
      SystemMessage::Unwatch { watchee, watcher } => self.handle_unwatch(watchee, watcher).await,
      SystemMessage::Failed {
        ref mut child_ref,
        cause,
      } => self.handle_failed(child_ref, cause).await,
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
