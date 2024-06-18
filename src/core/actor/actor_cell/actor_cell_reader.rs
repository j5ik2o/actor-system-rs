use crate::core::actor::actor_context::{ActorContext, ActorContextRef};
use crate::core::actor::actor_path::{ActorPath, ActorPathBehavior};
use crate::core::actor::actor_ref::{InternalActorRef, LocalActorRef};
use crate::core::actor::supervisor_strategy::SupervisorStrategy;
use crate::core::actor::{Actor, ActorError, AnyActorReader, AnyActorRef, SysTell};
use crate::core::dispatch::any_message::AnyMessage;
use crate::core::dispatch::mailbox::system_message::SystemMessage;
use crate::core::dispatch::mailbox::Mailbox;
use crate::core::dispatch::message::AutoReceivedMessage;
use async_trait::async_trait;
use std::sync::Arc;
use tokio::sync::Notify;

#[derive(Debug)]
pub struct ActorCellReader<A: Actor> {
  actor: A,
  mailbox: Mailbox,
  actor_context_ref_opt: Option<ActorContextRef>,
  terminate_notify: Arc<Notify>,
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

  fn get_actor_context_ref(&self) -> ActorContextRef {
    self.actor_context_ref_opt.as_ref().unwrap().clone()
  }

  async fn get_actor_context(&self) -> ActorContext {
    let actor_context_ref = self.get_actor_context_ref();
    let actor_context = actor_context_ref.upgrade().await.as_ref().unwrap().clone();
    actor_context
  }

  async fn self_ref(&self) -> InternalActorRef {
    self.get_actor_context().await.internal_self_ref().await
  }

  async fn suspend_self(&mut self) {
    let actor_context = self.get_actor_context().await;
    let mut self_ref = actor_context.internal_self_ref().await;
    self_ref.suspend().await;
  }

  async fn suspend_children(&mut self) {
    let actor_context = self.get_actor_context().await;
    let child_refs = actor_context.get_child_refs().await;
    for mut child_ref in child_refs {
      child_ref.suspend().await;
    }
  }

  async fn handle_invoke_failure(&mut self, cause: Arc<ActorError>) {
    log::error!("handle_invoke_failure: {:?}", cause);
    self.suspend_self().await;
    self.suspend_children().await;

    let mut parent_ref_opt = match self.get_parent().await {
      Some(parent_ref) if parent_ref.path().is_child() => Some(parent_ref),
      _ => None,
    };
    if let Some(parent_ref) = &mut parent_ref_opt {
      parent_ref
        .sys_tell(SystemMessage::failed(self.self_ref().await, cause))
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

  async fn handle_watch(&mut self, watchee: InternalActorRef, watcher: InternalActorRef) {
    log::debug!(
      "Watch: watchee = {}, watcher = {}, path = {}, suspend = {}",
      watchee,
      watcher,
      self.path().await,
      self.mailbox.is_suspend().await
    );
  }

  async fn handle_unwatch(&mut self, watchee: InternalActorRef, watcher: InternalActorRef) {
    log::debug!(
      "Unwatch: watchee = {}, watcher = {}, path = {}, suspend = {}",
      watchee,
      watcher,
      self.path().await,
      self.mailbox.is_suspend().await
    );
  }

  async fn handle_failed(&mut self, child_ref: &mut InternalActorRef, cause: Arc<ActorError>) {
    let actor_context = self.get_actor_context().await;
    log::debug!(
      "Failed: child_ref = {}, cause = {}, path = {}, suspend = {}",
      child_ref,
      cause,
      self.path().await,
      self.mailbox.is_suspend().await
    );
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
        let self_ref = self.get_actor_context().await.internal_self_ref().await;
        parent_ref
          .tell_any(AnyMessage::new(AutoReceivedMessage::terminated(self_ref, false, false)))
          .await;
      }
      self.actor.around_post_stop(actor_context.clone()).await;
      self.mailbox.become_closed().await;
      if parent_ref_opt.is_none() {
        self.terminate_notify.notify_waiters();
      }
    }
  }

  async fn handle_supervise(&mut self, child: LocalActorRef, r#async: bool) {
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
impl<A: Actor + 'static> AnyActorReader for ActorCellReader<A> {
  async fn path(&self) -> ActorPath {
    self.get_actor_context().await.internal_self_ref().await.path().clone()
  }

  async fn get_parent(&self) -> Option<InternalActorRef> {
    let result = self.get_actor_context().await.get_parent_context().await;
    match result {
      Some(parent_context) => Some(parent_context.internal_self_ref().await.clone()),
      None => None,
    }
  }

  fn set_actor_context_ref(&mut self, actor_context_ref: ActorContextRef) {
    self.actor_context_ref_opt = Some(actor_context_ref);
  }

  async fn child_terminated(&mut self, child: InternalActorRef) {
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
      if let Err(error) = self.actor.receive(actor_context, message).await {
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