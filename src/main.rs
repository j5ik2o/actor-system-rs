use std::env;
use std::thread::sleep;
use std::time::Duration;

use actor_system_rs::core::actor::actor_context::ActorContext;
use actor_system_rs::core::actor::actor_system::ActorSystem;
use actor_system_rs::core::actor::props::Props;
use actor_system_rs::core::actor::Actor;
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
#[derive(Debug)]
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

  async fn receive(&mut self, ctx: ActorContext, message: Self::M) {
    log::debug!("receive: a message on {}, {:?}", ctx.self_path().await, message);
    if message.value == self.answer {
      log::debug!("receive: the answer to life, the universe, and everything");
    }
    ctx.terminate_system().await;
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
    sleep(Duration::from_secs(1));
    actor_ref.tell(MyMessage { value: 42 }).await;
  });
  system.when_terminated().await;
}
