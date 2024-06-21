use std::collections::HashSet;
use std::sync::Arc;

use async_trait::async_trait;
use tokio::sync::Notify;

use crate::core::actor::actor_context::{ActorContext, ActorContextRef};
use crate::core::actor::actor_path::{ActorPath, ActorPathBehavior};
use crate::core::actor::actor_ref::{InternalActorRef, LocalActorRef};
use crate::core::actor::children::children_container::ChildrenContainer;
use crate::core::actor::children::children_refs::SuspendReason;
use crate::core::actor::props::Props;
use crate::core::actor::supervisor_strategy::SupervisorStrategy;
use crate::core::actor::{Actor, ActorError, AnyActorReader, AnyActorRef, SysTell};
use crate::core::dispatch::any_message::AnyMessage;
use crate::core::dispatch::dispatcher::Dispatcher;
use crate::core::dispatch::envelope::Envelope;
use crate::core::dispatch::mailbox::system_message::SystemMessage;
use crate::core::dispatch::mailbox::Mailbox;
use crate::core::dispatch::message::AutoReceivedMessage;

#[derive(Debug, Clone)]
pub enum FailedInfo {
  NoFailedInfo,
  FailedRef(InternalActorRef),
  FailedFatally,
}

#[derive(Debug)]
pub struct ActorCellReader<A: Actor> {
  props: Props<A>,
  actor_opt: Option<A>,
  mailbox: Mailbox,
  actor_context_ref_opt: Option<ActorContextRef>,
  terminate_notify: Arc<Notify>,
  failed_info: FailedInfo,
  current_message: Option<Envelope>,
  watched_by: HashSet<InternalActorRef>,
}

impl<A: Actor + 'static> ActorCellReader<A> {
  pub fn new(props: Props<A>, mailbox: Mailbox, terminate_notify: Arc<Notify>) -> Self {
    Self {
      props,
      actor_opt: None,
      mailbox,
      actor_context_ref_opt: None,
      terminate_notify,
      failed_info: FailedInfo::NoFailedInfo,
      current_message: None,
      watched_by: HashSet::new(),
    }
  }

  pub fn mailbox(&self) -> &Mailbox {
    &self.mailbox
  }

  pub fn mailbox_mut(&mut self) -> &mut Mailbox {
    &mut self.mailbox
  }

  pub(crate) async fn swap_mailbox(&mut self, mailbox: Mailbox) -> Mailbox {
    let old_mailbox = std::mem::replace(&mut self.mailbox, mailbox);
    old_mailbox
  }

  async fn get_dispatcher(&self) -> Dispatcher {
    self.get_actor_context().await.get_dispatcher().await.clone()
  }

  fn is_failed(&self) -> bool {
    match self.failed_info {
      FailedInfo::FailedRef(_) => true,
      _ => false,
    }
  }

  fn is_failed_fatally(&self) -> bool {
    match self.failed_info {
      FailedInfo::FailedFatally => true,
      _ => false,
    }
  }

  fn perpetrator(&self) -> Option<InternalActorRef> {
    match &self.failed_info {
      FailedInfo::FailedRef(ref actor_ref) => Some(actor_ref.clone()),
      _ => None,
    }
  }

  fn set_field(&mut self, child_ref: InternalActorRef) {
    self.failed_info = match self.failed_info {
      FailedInfo::FailedFatally => FailedInfo::FailedFatally,
      _ => FailedInfo::FailedRef(child_ref),
    }
  }

  fn set_failed_fatally(&mut self) {
    self.failed_info = match self.failed_info {
      FailedInfo::FailedRef(_) => FailedInfo::FailedFatally,
      _ => FailedInfo::FailedRef(self.perpetrator().unwrap()),
    }
  }

  fn clear_failed(&mut self) {
    self.failed_info = match &self.failed_info {
      FailedInfo::FailedRef(_) => FailedInfo::NoFailedInfo,
      other => other.clone(),
    }
  }

  fn actor(&self) -> &A {
    self.actor_opt.as_ref().unwrap()
  }

  fn actor_mut(&mut self) -> &mut A {
    self.actor_opt.as_mut().unwrap()
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

  fn new_actor(&mut self) -> &mut A {
    let result = self.props.create();
    self.actor_opt = Some(result);
    self.actor_opt.as_mut().unwrap()
  }

  // ---

  async fn suspend_non_recursive(&mut self) {
    self.get_dispatcher().await.suspend(self).await;
  }

  async fn resume_non_recursive(&mut self) {
    self.get_dispatcher().await.resume(self).await;
  }

  async fn suspend_children(&mut self) {
    let actor_context = self.get_actor_context().await;
    let child_refs = actor_context.get_children_refs().await;
    for mut child_ref in child_refs.children().await.iter_mut() {
      child_ref.suspend().await;
    }
  }

  async fn resume_children(&mut self, caused_by_failure: Arc<ActorError>) {
    let actor_context = self.get_actor_context().await;
    let child_refs = actor_context.get_children_refs().await;
    for mut child_ref in child_refs.children().await.iter_mut() {
      child_ref.resume(caused_by_failure.clone()).await;
    }
  }

  async fn stop_children(&mut self) {
    let actor_context = self.get_actor_context().await;
    let child_refs = actor_context.get_children_refs().await;
    for mut child_ref in child_refs.children().await.iter_mut() {
      child_ref.stop().await;
    }
  }

  // ---

  async fn handle_invoke_failure(&mut self, cause: Arc<ActorError>) {
    log::error!("handle_invoke_failure: {:?}", cause);
    if !self.is_failed() {
      self.suspend_non_recursive().await;
      match &self.current_message {
        Some(Envelope { sender, .. }) => {
          self.set_field(sender.clone());
        }
        _ => {
          self.set_field(self.self_ref().await);
        }
      }
      self.suspend_children().await;
      self.send_failed_message(cause).await;
      self.stop_children().await;
      self.finish_terminate().await;
    }
  }

  // ---

  async fn create(&mut self, cause: Option<Arc<ActorError>>) -> Result<(), Arc<ActorError>> {
    let actor_context = self.get_actor_context().await;
    log::debug!(
      "Create: path = {}, suspend = {}",
      self.path().await,
      self.mailbox.is_suspend().await
    );
    if self.actor_opt.is_some() {
      self.clear_failed();
      self.set_failed_fatally();
      self.actor_opt = None;
    }
    if cause.is_some() {
      return Err(cause.unwrap());
    }
    let instance = self.new_actor();
    instance.around_pre_start(actor_context).await;
    Ok(())
  }

  fn clear_actor_fields(&mut self) {
    self.current_message = None;
  }

  async fn fault_recreate(&mut self, cause: Arc<ActorError>) {
    log::debug!(
      "Recreate: cause = {}, path = {}, suspend = {}",
      cause,
      self.path().await,
      self.mailbox.is_suspend().await
    );
    let actor_context = self.get_actor_context().await;
    if self.actor_opt.is_none() {
      self.fault_create().await;
    } else if actor_context.get_children_refs().await.is_normal().await {
      let failed_actor = &self.actor_opt;
      if failed_actor.is_some() {
        if !self.is_failed_fatally() {
          let msg = match &mut self.current_message {
            Some(Envelope { message: msg, .. }) => Some(msg.take::<A::M>().unwrap()),
            None => None,
          };
          self
            .actor_opt
            .as_mut()
            .unwrap()
            .around_pre_restart(actor_context, cause.clone(), msg)
            .await;
          self.clear_actor_fields();
        }
      }
      if self
        .set_children_termination_reason(SuspendReason::Recreation { cause: cause.clone() })
        .await
      {
        self.finish_recreate(cause).await;
      }
    } else {
      self.fault_resume(Some(cause)).await;
    }
  }

  async fn set_children_termination_reason(&self, reason: SuspendReason) -> bool {
    let mut actor_context = self.get_actor_context().await;
    let mut child_refs = actor_context.get_children_refs().await;
    if child_refs.is_terminating().await {
      let mut c = child_refs.deep_copy().await;
      c.set_suspend_reason(reason).await;
      let new_child_refs = std::mem::replace(&mut child_refs, c);
      actor_context.set_children_refs(new_child_refs).await;
      true
    } else {
      false
    }
  }

  async fn fault_suspend(&mut self) {
    log::debug!(
      "Suspend: path = {}, suspend = {}",
      self.path().await,
      self.mailbox.is_suspend().await
    );
    self.suspend_non_recursive().await;
    self.suspend_children().await;
  }

  async fn fault_resume(&mut self, cause: Option<Arc<ActorError>>) {
    log::debug!(
      "Resume: case = {:?}, path = {}, suspend = {}",
      cause,
      self.path().await,
      self.mailbox.is_suspend().await
    );
    if self.actor_opt.is_none() {
      self.fault_create().await;
    // FIXME: recursive call on async fn
    // } else if self.is_failed_fatally() && cause.is_some() {
    //   self.fault_recreate(cause.unwrap()).await;
    } else {
      self.resume_non_recursive().await;
      if cause.is_some() {
        self.clear_failed();
      }
      self.resume_children(cause.unwrap()).await;
    }
  }

  async fn terminate(&mut self) {
    log::debug!(
      "Terminate: path = {}, suspend = {}",
      self.path().await,
      self.mailbox.is_suspend().await
    );
    self.stop_children().await;
    let mut actor_context = self.get_actor_context().await;
    actor_context.terminate_children_refs().await;
    self.finish_terminate().await;
  }

  async fn add_watcher(&mut self, watchee: InternalActorRef, watcher: InternalActorRef) {
    log::debug!(
      "Watch: watchee = {}, watcher = {}, path = {}, suspend = {}",
      watchee,
      watcher,
      self.path().await,
      self.mailbox.is_suspend().await
    );
    if !self.watched_by.contains(&watcher) {
      self.watched_by.insert(watcher);
    }
  }

  async fn remove_watcher(&mut self, watchee: InternalActorRef, watcher: InternalActorRef) {
    log::debug!(
      "Unwatch: watchee = {}, watcher = {}, path = {}, suspend = {}",
      watchee,
      watcher,
      self.path().await,
      self.mailbox.is_suspend().await
    );
    if self.watched_by.contains(&watcher) {
      self.watched_by.remove(&watcher);
    }
  }

  async fn handle_failure(&mut self, failed: SystemMessage, child_ref: InternalActorRef, cause: Arc<ActorError>) {
    self.current_message = Some(Envelope::new(AnyMessage::new(failed), child_ref.clone()));
    let actor_context = self.get_actor_context().await;
    log::debug!(
      "Failed: child_ref = {}, cause = {}, path = {}, suspend = {}",
      child_ref,
      cause,
      self.path().await,
      self.mailbox.is_suspend().await
    );
    if !self
      .actor()
      .supervisor_strategy()
      .await
      .handle_failure(actor_context, false, child_ref.clone(), cause.clone(), vec![])
      .await
    {
      self.handle_invoke_failure(cause).await;
    }
  }

  // ---

  async fn fault_create(&mut self) {
    self.stop_children().await;
    self.finish_create().await;
  }

  async fn finish_create(&mut self) {
    self.resume_non_recursive().await;
    self.clear_failed();
    match self.create(None).await {
      Ok(_) => {}
      Err(cause) => {
        self.handle_invoke_failure(cause).await;
      }
    }
  }

  async fn finish_recreate(&mut self, cause: Arc<ActorError>) {
    let actor_context = self.get_actor_context().await;
    self.resume_non_recursive().await;
    let mut fresh_actor = self.new_actor();
    fresh_actor.around_pre_restart(actor_context, cause, None).await;
  }

  async fn finish_terminate(&mut self) {
    let actor_context = self.get_actor_context().await;
    if !actor_context.is_child_empty().await {
      return;
    }

    self.actor_mut().around_post_stop(actor_context.clone()).await;
    self.get_dispatcher().await.detach(self).await;
    let parent_context = actor_context.get_parent_context().await.unwrap();
    let self_ref = actor_context.internal_self_ref().await;
    parent_context.remove_child(self_ref).await;

    // TODO: sendSystemMessage(DeathWatchNotification(self, existenceConfirmed = true, addressTerminated = false)))
    // TODO: tellWatchersWeDied
    // TODO: unwatchWatchedActors
    let parent_ref_opt = self.send_terminated_message().await;
    if parent_ref_opt.is_none() {
      self.terminate_notify.notify_waiters();
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

  // ---

  async fn send_failed_message(&mut self, cause: Arc<ActorError>) {
    let mut parent_ref_opt = match self.get_parent().await {
      Some(parent_ref) => Some(parent_ref),
      _ => None,
    };
    if let Some(parent_ref) = &mut parent_ref_opt {
      parent_ref
        .sys_tell(SystemMessage::failed(self.self_ref().await, cause))
        .await;
    }
  }

  async fn send_terminated_message(&mut self) -> Option<InternalActorRef> {
    let actor_context = self.get_actor_context().await;
    let self_ref = actor_context.internal_self_ref().await;
    for watcher in self.watched_by.iter() {
      watcher
        .tell_any(AnyMessage::new(AutoReceivedMessage::terminated(
          self_ref.clone(),
          false,
          false,
        )))
        .await;
    }
    if let Some(parent_ref) = &self.get_parent().await {
      parent_ref
        .tell_any(AnyMessage::new(AutoReceivedMessage::terminated(self_ref, false, false)))
        .await;
      Some(parent_ref.clone())
    } else {
      None
    }
  }
}

#[async_trait]
impl<A: Actor + 'static> AnyActorReader for ActorCellReader<A> {
  async fn path(&self) -> ActorPath {
    self.get_actor_context().await.internal_self_ref().await.path().clone()
  }

  async fn get_parent(&self) -> Option<InternalActorRef> {
    match self.get_actor_context().await.get_parent_context().await {
      Some(parent_context) => {
        let parent_ref = parent_context.internal_self_ref().await;
        log::debug!("get_parent.path(): {}", parent_ref.path());
        log::debug!("get_parent.path().name(): {}", parent_ref.path().name());
        log::debug!("get_parent.path().is_child(): {}", parent_ref.path().is_child());
        if parent_ref.path().is_child() && parent_ref.path().name() != "/" {
          Some(parent_ref)
        } else {
          None
        }
      }
      None => None,
    }
  }

  fn set_actor_context_ref(&mut self, actor_context_ref: ActorContextRef) {
    self.actor_context_ref_opt = Some(actor_context_ref);
  }

  async fn child_terminated(&mut self, child: InternalActorRef) {
    log::debug!("child_terminated: {}", child.path());
    let actor_context = self.get_actor_context().await;
    actor_context.remove_child(child.clone()).await;
    self.actor_mut().child_terminated(actor_context.clone(), child).await;
    if actor_context.is_child_empty().await {
      self.actor_mut().all_children_terminated(actor_context.clone()).await;
    }
  }

  // ---

  async fn invoke(&mut self, mut message: AnyMessage) {
    if let Ok(message) = message.take::<A::M>() {
      let actor_context = self.get_actor_context().await;
      if let Err(error) = self.actor_mut().receive(actor_context, message).await {
        self.handle_invoke_failure(Arc::new(error)).await;
      }
    }
  }

  async fn system_invoke(&mut self, mut system_message: SystemMessage) {
    log::debug!("system_invoke: {:?}", system_message);
    match system_message {
      SystemMessage::Create { failure } => {
        let _ = self.create(failure).await.unwrap();
      }
      SystemMessage::Recreate { cause } => self.fault_recreate(cause).await,
      SystemMessage::Suspend => self.fault_suspend().await,
      SystemMessage::Resume {
        caused_by_failure: cause,
      } => self.fault_resume(Some(cause)).await,
      SystemMessage::Terminate => self.terminate().await,
      SystemMessage::Supervise { .. } => {} // use RepointableActorRef
      SystemMessage::Watch { watchee, watcher } => self.add_watcher(watchee, watcher).await,
      SystemMessage::Unwatch { watchee, watcher } => self.remove_watcher(watchee, watcher).await,
      ref msg @ SystemMessage::Failed {
        ref child_ref,
        ref cause,
      } => self.handle_failure(msg.clone(), child_ref.clone(), cause.clone()).await,
    }
  }

  async fn get_terminate_notify(&self) -> Arc<Notify> {
    self.terminate_notify.clone()
  }

  fn supervisor_strategy(&self) -> Arc<Box<dyn SupervisorStrategy>> {
    todo!()
  }
}
