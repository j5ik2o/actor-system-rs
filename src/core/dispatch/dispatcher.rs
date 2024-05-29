use std::sync::Arc;

use tokio::sync::Mutex;
use crate::core::actor::actor_cells::ActorCells;

use crate::core::dispatch::mailbox::Mailbox;

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

  pub async fn run(&self, actor_cells: ActorCells) {
    let mailboxes = self.mailboxes.lock().await.clone();
    for mut mailbox in mailboxes {
      let actor_cells = actor_cells.clone();
      tokio::spawn(async move {
        mailbox.execute(actor_cells).await;
      });
    }
  }

  pub async fn stop(&self) {
    // let mailboxes = self.mailboxes.lock().await.clone();
    // for mut mailbox in mailboxes {
    //   // mailbox.sender_mut().await.disconnect();
    // }
  }
}
