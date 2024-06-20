use std::fmt::Debug;
use std::sync::Arc;

use crate::core::actor::actor_ref::{InternalActorRef, LocalActorRef};
use crate::core::actor::ActorError;
use crate::core::util::element::Element;

#[derive(Debug, Clone)]
pub enum SystemMessage {
  Create {
    failure: Option<Arc<ActorError>>,
  },
  Recreate {
    cause: Arc<ActorError>,
  },
  Suspend,
  Resume {
    caused_by_failure: Arc<ActorError>,
  },
  Terminate,
  Supervise {
    child: InternalActorRef,
    r#async: bool,
  },
  Watch {
    watchee: InternalActorRef,
    watcher: InternalActorRef,
  },
  Unwatch {
    watchee: InternalActorRef,
    watcher: InternalActorRef,
  },
  Failed {
    child_ref: InternalActorRef,
    cause: Arc<ActorError>,
  },
}

impl Element for SystemMessage {}

unsafe impl Send for SystemMessage {}
unsafe impl Sync for SystemMessage {}

impl SystemMessage {
  pub fn create() -> Self {
    Self::Create { failure: None }
  }

  pub fn recreate(cause: Arc<ActorError>) -> Self {
    Self::Recreate { cause }
  }

  pub fn suspend() -> Self {
    Self::Suspend
  }

  pub fn resume(caused_by_failure: Arc<ActorError>) -> Self {
    Self::Resume { caused_by_failure }
  }

  pub fn terminate() -> Self {
    Self::Terminate
  }

  pub fn supervise(child: InternalActorRef, r#async: bool) -> Self {
    Self::Supervise { child, r#async }
  }

  pub fn watch(watchee: InternalActorRef, watcher: InternalActorRef) -> Self {
    Self::Watch { watchee, watcher }
  }

  pub fn unwatch(watchee: InternalActorRef, watcher: InternalActorRef) -> Self {
    Self::Unwatch { watchee, watcher }
  }

  pub fn failed(child_ref: InternalActorRef, cause: Arc<ActorError>) -> Self {
    Self::Failed { child_ref, cause }
  }
}
