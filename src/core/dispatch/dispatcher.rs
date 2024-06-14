use std::sync::Arc;
use tokio::runtime::Runtime;

use tokio::sync::Mutex;

use crate::core::dispatch::mailbox::Mailbox;

#[derive(Debug, Clone)]
pub struct Dispatcher {
  runtime: Option<Arc<Runtime>>,
  mailboxes: Arc<Mutex<Vec<Mailbox>>>,
}

impl Dispatcher {
  pub fn new() -> Self {
    Self {
      runtime: None,
      mailboxes: Arc::new(Mutex::new(Vec::new())),
    }
  }

  pub fn with_runtime(runtime: Arc<Runtime>) -> Self {
    Self {
      runtime: Some(runtime),
      mailboxes: Arc::new(Mutex::new(Vec::new())),
    }
  }

  pub async fn register(&self, mailbox: Mailbox) {
    let mut mailboxes = self.mailboxes.lock().await;
    mailboxes.push(mailbox);
  }

  pub async fn run(&self) {
    let mailboxes = self.mailboxes.lock().await.clone();
    for mut mailbox in mailboxes {
      if let Some(rt) = &self.runtime {
        rt.spawn(async move {
          mailbox.execute().await;
        });
      } else {
        tokio::spawn(async move {
          mailbox.execute().await;
        });
      }
    }
  }
}
