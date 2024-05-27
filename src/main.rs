use std::env;

use async_trait::async_trait;

use simple_actor_rs::core::actor::actor_context::ActorContext;
use simple_actor_rs::core::actor::actor_system::ActorSystem;
use simple_actor_rs::core::actor::Actor;
use simple_actor_rs::core::dispatch::message::Message;
use simple_actor_rs::core::util::element::Element;

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
pub struct MyActor;

#[async_trait]
impl Actor for MyActor {
  type M = MyMessage;

  async fn receive(&mut self, ctx: ActorContext<Self::M>, message: Self::M) {
    log::debug!("received a message on {}, {:?}", ctx.self_ref.path(), message);
    ctx.terminate_system().await;
  }
}

#[tokio::main]
async fn main() {
  let _ = env::set_var("RUST_LOG", "debug");
  let _ = env_logger::init();

  let system = ActorSystem::new();
  let actor_path = "my_actor".to_string();

  let actor_ref = system.actor_of(actor_path.clone(), MyActor).await;

  actor_ref.tell(&system, MyMessage { value: 32 }).await;

  system.when_terminated().await;
}
