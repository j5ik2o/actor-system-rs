use crate::core::actor::actor_context::ActorContextRef;
use crate::core::actor::actor_ref::UntypedActorRef;
use crate::core::actor::{AnyActorRef, SysTell};
use crate::core::dispatch::any_message::AnyMessage;
use crate::core::dispatch::mailbox::system_message::SystemMessage;
use std::collections::{HashMap, HashSet};

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
