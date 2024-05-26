use std::sync::Arc;

use futures::StreamExt;
use tokio::sync::Mutex;

use crate::core::dispatch::mailbox::Mailbox;
use crate::core::util::queue::QueueReadBehavior;

#[derive(Debug, Clone)]
pub struct Dispatcher {
  mailboxes: Arc<Mutex<Vec<Mailbox>>>,
}

impl Dispatcher {
  pub fn new() -> Self {
    Self {
      mailboxes: Arc::new(Mutex::new(Vec::new())),
    }
  }

  pub async fn register(&self, mailbox: Mailbox) {
    let mut mailboxes = self.mailboxes.lock().await;
    mailboxes.push(mailbox);
  }

  pub async fn run(&self) {
    let mailboxes = self.mailboxes.lock().await.clone();
    for mailbox in mailboxes {
      let receiver = mailbox.receiver().await.clone();
      let actor = mailbox.actor().await.clone();
      tokio::spawn(async move {
        let mut receiver = receiver.lock().await;
        while let Some(message) = receiver.next().await {
          if let Some(actor) = actor.as_ref() {
            actor.lock().await.receive(message).await;
          }
        }
      });
    }
  }

  pub async fn stop(&self) {
    let mailboxes = self.mailboxes.lock().await.clone();
    for mut mailbox in mailboxes {
      mailbox.sender_mut().await.disconnect();
    }
  }
}
