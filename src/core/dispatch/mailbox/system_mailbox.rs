use crate::core::actor::AnyActor;
use crate::core::dispatch::mailbox::system_message::SystemMessage;
use futures::channel::mpsc::{unbounded, UnboundedReceiver, UnboundedSender};
use std::sync::Arc;
use tokio::sync::Mutex;

pub struct SystemMailbox {
  pub(crate) sender: UnboundedSender<SystemMessage>,
  pub(crate) receiver: Arc<Mutex<UnboundedReceiver<SystemMessage>>>,
  actor: Arc<Option<Arc<Mutex<Box<dyn AnyActor>>>>>,
}

impl SystemMailbox {
  pub fn new() -> Self {
    let (sender, receiver) = unbounded();
    Self {
      sender,
      receiver: Arc::new(Mutex::new(receiver)),
      actor: Arc::new(None),
    }
  }

  pub fn set_actor(&mut self, actor: Arc<Mutex<Box<dyn AnyActor>>>) {
    self.actor = Arc::new(Some(actor));
  }
}
