use crate::core::actor::actor_ref::UntypedActorRef;
use crate::core::actor::ActorError;
use std::error::Error;
use std::fmt::Debug;
use std::sync::Arc;

use crate::core::util::element::Element;

#[derive(Debug, Clone)]
pub enum SystemMessage {
  Create,
  Suspend,
  Resume,
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
  Recreate {
    cause: Arc<ActorError>,
  },
  Terminate,
}

impl Element for SystemMessage {}

unsafe impl Send for SystemMessage {}
unsafe impl Sync for SystemMessage {}
