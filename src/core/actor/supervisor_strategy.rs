use crate::core::actor::actor_context::ActorContext;
use crate::core::actor::actor_ref::UntypedActorRef;
use std::error::Error;
use std::sync::Arc;

enum Directive {
  Resume,
  Restart,
  Stop,
  Escalate,
}

type Decider = Box<dyn Fn(Arc<Box<dyn Error + Send + Sync>>) -> Directive>;

trait SupervisorStrategy {
  fn decider(&self) -> Decider;
  async fn handle_child_terminated(&self, ctx: ActorContext, child: UntypedActorRef, children: Vec<UntypedActorRef>);

  async fn process_failure(
    &self,
    ctx: ActorContext,
    restart: bool,
    child: UntypedActorRef,
    cause: Arc<Box<dyn Error + Send + Sync>>,
    children: Vec<UntypedActorRef>,
  );
}
