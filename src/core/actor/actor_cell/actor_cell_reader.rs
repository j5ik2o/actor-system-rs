use crate::core::actor::actor_context::{ActorContext, ActorContextRef};
use crate::core::actor::actor_path::ActorPathBehavior;
use crate::core::actor::actor_ref::UntypedActorRef;
use crate::core::actor::{Actor, ActorError, AnyActorReader, AnyActorRef, SysTell};
use crate::core::dispatch::any_message::AnyMessage;
use crate::core::dispatch::mailbox::system_message::SystemMessage;
use crate::core::dispatch::mailbox::Mailbox;
use crate::core::dispatch::message::AutoReceivedMessage;
use std::sync::Arc;
use tokio::sync::Notify;

#[derive(Debug)]
pub struct ActorCellReader<A: Actor> {
  pub(crate) actor: A,
  mailbox: Mailbox,
  pub(crate) actor_context_ref_opt: Option<ActorContextRef>,
  pub(crate) terminate_notify: Arc<Notify>,
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

  pub(crate) async fn handle_invoke_failure(&mut self, cause: Arc<ActorError>) {
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

  pub(crate) async fn handle_create(&mut self) {
    let actor_context = self.get_actor_context().await;
    log::debug!(
      "Create: path = {}, suspend = {}",
      self.path().await,
      self.mailbox.is_suspend().await
    );
    self.actor.around_pre_start(actor_context).await;
  }

  pub(crate) async fn handle_suspend(&mut self) {
    log::debug!(
      "Suspend: path = {}, suspend = {}",
      self.path().await,
      self.mailbox.is_suspend().await
    );
    self.mailbox.suspend().await;
  }

  pub(crate) async fn handle_resume(&mut self, cause: Arc<ActorError>) {
    log::debug!(
      "Resume: case = {}, path = {}, suspend = {}",
      cause,
      self.path().await,
      self.mailbox.is_suspend().await
    );
    self.mailbox.resume().await;
  }

  pub(crate) async fn handle_watch(&mut self, watchee: UntypedActorRef, watcher: UntypedActorRef) {
    log::debug!(
      "Watch: watchee = {}, watcher {}, path = {}, suspend = {}",
      watchee,
      watcher,
      self.path().await,
      self.mailbox.is_suspend().await
    );
  }

  pub(crate) async fn handle_unwatch(&mut self, watchee: UntypedActorRef, watcher: UntypedActorRef) {
    log::debug!(
      "Unwatch: watchee = {}, watcher {}, path = {}, suspend = {}",
      watchee,
      watcher,
      self.path().await,
      self.mailbox.is_suspend().await
    );
  }

  pub(crate) async fn handle_failed(&mut self, child_ref: &mut UntypedActorRef, cause: Arc<ActorError>) {
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

  pub(crate) async fn handle_recreate(&mut self, cause: Arc<ActorError>) {
    log::debug!(
      "Recreate: cause = {}, path = {}, suspend = {}",
      cause,
      self.path().await,
      self.mailbox.is_suspend().await
    );
  }

  pub(crate) async fn handle_terminated(&mut self) {
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

  pub(crate) async fn handle_supervise(&mut self, child: UntypedActorRef, r#async: bool) {
    log::debug!(
      "Supervise: child = {}, async = {}, path = {}, suspend = {}",
      child,
      r#async,
      self.path().await,
      self.mailbox.is_suspend().await
    );
  }
}
