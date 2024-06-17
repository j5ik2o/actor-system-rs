use std::fmt::Debug;
use std::sync::Arc;

use crate::core::actor::actor_ref::UntypedActorRef;
use crate::core::actor::ActorError;
use crate::core::util::element::Element;

#[derive(Debug, Clone)]
pub enum SystemMessage {
  Create,
  Recreate {
    cause: Arc<ActorError>,
  },
  Suspend,
  Resume {
    caused_by_failure: Arc<ActorError>,
  },
  Terminate,
  Supervise {
    child: UntypedActorRef,
    r#async: bool,
  },
  Watch {
    watchee: UntypedActorRef,
    watcher: UntypedActorRef,
  },
  Unwatch {
    watchee: UntypedActorRef,
    watcher: UntypedActorRef,
  },
  Failed {
    child_ref: UntypedActorRef,
    cause: Arc<ActorError>,
  },
}

impl Element for SystemMessage {}

unsafe impl Send for SystemMessage {}
unsafe impl Sync for SystemMessage {}
