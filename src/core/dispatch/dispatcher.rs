use std::sync::Arc;

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
      let mut reader = mailbox.queue_reader().await;
      let actor = mailbox.actor().await.clone();
      tokio::spawn(async move {
        while let Ok(Some(message)) = reader.poll().await {
          if let Some(actor) = actor.as_ref() {
            actor.lock().await.invoke(message).await;
          }
        }
      });
    }
  }

  pub async fn stop(&self) {
    let mailboxes = self.mailboxes.lock().await.clone();
    for mut mailbox in mailboxes {
      // mailbox.sender_mut().await.disconnect();
    }
  }
}
