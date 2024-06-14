use crate::core::actor::actor_ref::UntypedActorRef;
use std::fmt::Debug;

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
  Terminate,
}

impl Element for SystemMessage {}
