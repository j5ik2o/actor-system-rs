use std::collections::{HashMap, HashSet};

use crate::core::actor::actor_context::{ActorContext, ActorContextRef};
use crate::core::actor::actor_ref::{InternalActorRef, LocalActorRef};
use crate::core::actor::{AnyActorRef, SysTell};
use crate::core::dispatch::any_message::AnyMessage;
use crate::core::dispatch::mailbox::system_message::SystemMessage;
use crate::core::dispatch::message::AutoReceivedMessage;

pub struct DeathWatch {
  actor_context_ref: ActorContextRef,
  watching: HashMap<InternalActorRef, Option<AnyMessage>>,
  watched_by: HashSet<InternalActorRef>,
  terminated_queued: HashMap<InternalActorRef, Option<AnyMessage>>,
}

impl DeathWatch {
  pub fn new(actor_context_ref: ActorContextRef) -> Self {
    Self {
      actor_context_ref,
      watching: HashMap::new(),
      watched_by: HashSet::new(),
      terminated_queued: HashMap::new(),
    }
  }

  pub(crate) fn get_actor_context_ref(&self) -> ActorContextRef {
    self.actor_context_ref.clone()
  }

  pub(crate) async fn get_actor_context(&self) -> ActorContext {
    let actor_context_ref = self.get_actor_context_ref();
    let actor_context = actor_context_ref.upgrade().await.as_ref().unwrap().clone();
    actor_context
  }

  async fn self_ref(&self) -> InternalActorRef {
    self.get_actor_context().await.self_ref().await
  }

  fn is_terminating(&self) -> bool {
    todo!()
  }

  fn is_watching(&self, subject: &InternalActorRef) -> bool {
    self.watching.contains_key(subject)
  }

  async fn update_watching(&mut self, subject: &InternalActorRef, message: Option<AnyMessage>) {
    if let Some(v) = self.watching.get_mut(subject) {
      *v = message;
    }
  }

  async fn check_watching_some(&self, subject: &InternalActorRef) {
    let previous = self.watching.get(&subject);
    if previous != None {
      panic!("Watched by: {}", subject.path());
    }
  }

  async fn send_terminated(&self, if_local: bool, watcher: LocalActorRef) {
    // if watcher.is_local() == if_local {
    //
    // }
  }
  pub(crate) async fn tell_watchers_we_died(&self) {
    if !self.watched_by.is_empty() {

    }
  }

  pub async fn watch(&mut self, subject: InternalActorRef) -> InternalActorRef {
    let self_ref = self.self_ref().await;
    if subject != self_ref {
      if !self.watching.contains_key(&subject) {
        subject
          .sys_tell(SystemMessage::watch(
            self_ref.clone(),
            self_ref.clone(),
          ))
          .await;
        self.update_watching(&subject, None).await;
      } else {
        self.check_watching_some(&subject).await;
      }
    }
    subject
  }

  pub async fn unwatch(&mut self, subject: InternalActorRef) -> InternalActorRef {
    let self_ref = self.actor_context_ref.upgrade().await.unwrap().self_ref().await;
    if subject != self_ref {
      if self.watching.contains_key(&subject) {
        subject
          .sys_tell(SystemMessage::unwatch(
            self_ref.clone(),
            self_ref,
          ))
          .await;
        self.watching.remove(&subject);
      }
    }
    self.terminated_queued.remove(&subject);
    subject
  }

  pub(crate) async fn watched_actor_terminated(
    &mut self,
    actor: InternalActorRef,
    existence_confirmed: bool,
    address_terminated: bool,
  ) {
    if self.watching.contains_key(&actor) {
      let optional_message = self.watching.remove(&actor).unwrap();
      let actor_context = self.get_actor_context().await;
      if !self.is_terminating() {
        actor
          .tell_any(AnyMessage::new(AutoReceivedMessage::Terminated {
            actor: actor_context.self_ref().await,
            existence_confirmed,
            address_terminated,
          }))
          .await;
        self.terminated_queued_for(actor, optional_message.clone());
      }
    }
  }

  fn terminated_queued_for(&mut self, subject: InternalActorRef, custom_message: Option<AnyMessage>) {
    if !self.terminated_queued.contains_key(&subject) {
      self.terminated_queued.insert(subject, custom_message);
    }
  }

  pub async fn add_watcher(&mut self, watchee: InternalActorRef, watcher: InternalActorRef) {
    let self_ref = self.get_actor_context().await.self_ref().await;
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
        "Invalid add_watcher: watchee = {}, watcher = {}",
        watchee,
        watcher
      );
    }
  }

  pub async fn rem_watcher(&mut self, watchee: InternalActorRef, watcher: InternalActorRef) {
    let self_ref = self.get_actor_context().await.self_ref().await;
    let watchee_self = watchee == self_ref;
    let watcher_self = watcher == self_ref;

    if watchee_self && !watcher_self {
      if self.watched_by.iter().any(|w| *w == watcher) {
        self.watched_by.remove(&watcher);
      }
    } else if !watchee_self && watcher_self {
      self.unwatch(watchee).await;
    } else {
      panic!(
        "Invalid rem_watcher: watchee = {}, watcher = {}",
        watchee.path(),
        watcher.path()
      );
    }
  }
}
