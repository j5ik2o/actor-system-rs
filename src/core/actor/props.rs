use std::sync::Arc;

use crate::core::actor::Actor;

pub struct Props<A: Actor + Send + Sync> {
  pub(crate) creator: Arc<dyn Fn() -> A>,
}

impl<A: Actor + Send + Sync> Clone for Props<A> {
  fn clone(&self) -> Self {
    Props {
      creator: Arc::clone(&self.creator),
    }
  }
}

impl<A: Actor + Send + Sync> Props<A> {
  pub fn new<F>(creator: F) -> Self
  where
    F: Fn() -> A + Send + Sync + 'static, {
    Props {
      creator: Arc::new(creator),
    }
  }

  pub fn create(&self) -> A {
    (self.creator)()
  }
}
