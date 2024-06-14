use std::env;
use std::error::Error;
use std::thread::sleep;
use std::time::Duration;

use actor_system_rs::core::actor::actor_context::ActorContext;
use actor_system_rs::core::actor::actor_system::ActorSystem;
use actor_system_rs::core::actor::props::Props;
use actor_system_rs::core::actor::{Actor, ActorError, SysTell};
use actor_system_rs::core::dispatch::message::Message;
use actor_system_rs::core::util::element::Element;

// メッセージ型の例
#[derive(Clone, Debug)]
pub struct MyMessage {
  pub value: i32,
}
unsafe impl Send for MyMessage {}
unsafe impl Sync for MyMessage {}
impl Element for MyMessage {}
impl Message for MyMessage {}

// アクターの例
#[derive(Debug, Clone)]
pub struct MyActor {
  answer: i32,
}

impl MyActor {
  pub fn new(answer: i32) -> Self {
    Self { answer }
  }
}

#[async_trait::async_trait]
impl Actor for MyActor {
  type M = MyMessage;

  async fn pre_start(&mut self, ctx: ActorContext) {
    log::debug!("pre_start: {}", ctx.self_path().await);
  }

  async fn all_children_terminated(&mut self, ctx: ActorContext) {
    log::debug!("all_children_terminated: {}", ctx.self_path().await);
    ctx.self_ref().await.stop().await;
  }

  async fn receive(&mut self, ctx: ActorContext, message: Self::M) -> Result<(), ActorError> {
    log::debug!("receive: a message on {}, {:?}", ctx.self_path().await, message);
    if message.value == self.answer {
      log::debug!("receive: the answer to life, the universe, and everything");
    }
    let child = ctx.actor_of(Props::new(|| EchoActor::new(1)), "echo").await;
    child.tell(MyMessage { value: 1 }).await;
    Ok(())
  }
}

#[derive(Debug, Clone)]
pub struct EchoActor {
  answer: i32,
}

impl EchoActor {
  pub fn new(answer: i32) -> Self {
    Self { answer }
  }
}

#[async_trait::async_trait]
impl Actor for EchoActor {
  type M = MyMessage;

  async fn pre_start(&mut self, ctx: ActorContext) {
    log::debug!("pre_start: {}", ctx.self_path().await);
  }

  async fn receive(&mut self, ctx: ActorContext, message: Self::M) -> Result<(), ActorError> {
    log::debug!("receive: a message on {}, {:?}", ctx.self_path().await, message);
    ctx.self_ref().await.stop().await;
    // ctx.terminate_system().await;
    Ok(())
  }
}

#[tokio::main]
async fn main() {
  let _ = env::set_var("RUST_LOG", "debug");
  let _ = env_logger::init();

  let mut system = ActorSystem::new().await;
  let props = Props::new(|| MyActor::new(42));
  let actor_ref = system.actor_of(props, "actor1").await;
  tokio::spawn(async move {
    tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
    actor_ref.tell(MyMessage { value: 42 }).await;
    tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
  });
  system.when_terminated().await;
}
