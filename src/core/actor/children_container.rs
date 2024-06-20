use crate::core::actor::actor_ref::InternalActorRef;
use crate::core::actor::child_restart_stats::ChildRestartStats;
use crate::core::actor::child_state::ChildState;

#[async_trait::async_trait]
pub trait ChildrenContainer {
  async fn add(&self, stats: ChildRestartStats) -> Self;
  async fn remove(&self, child: InternalActorRef) -> Self;
  async fn get_by_name(&self, name: &str) -> Option<ChildState>;
  async fn get_by_ref(&self, child: InternalActorRef) -> Option<ChildRestartStats>;
  async fn children(&self) -> Vec<InternalActorRef>;
  async fn stats(&self) -> Vec<ChildRestartStats>;
  async fn shall_die(&self, actor: InternalActorRef) -> Self;
  async fn reserve(&self, name: &str) -> Self;
  async fn un_reserve(&self, name: &str) -> Self;
  async fn is_normal(&self) -> bool;
  async fn is_terminating(&self) -> bool;
}
