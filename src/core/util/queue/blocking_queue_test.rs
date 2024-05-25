extern crate env_logger;

use std::env;
use std::sync::Arc;
use std::time::Duration;

use futures::StreamExt;
use serial_test::serial;
use tokio::sync::Mutex;
use tokio_condvar::Condvar;
use crate::core::util::queue::{create_queue, create_queue_with_elements, Queue, QueueBehavior, QueueSize, QueueType};
use crate::core::util::queue::blocking_queue::BlockingQueue;


fn init_logger() {
    env::set_var("RUST_LOG", "debug");
    // env::set_var("RUST_LOG", "trace");
    let _ = env_logger::try_init();
}

struct CountDownLatch {
    count: Mutex<usize>,
    condvar: Condvar,
}

impl CountDownLatch {
    fn new(count: usize) -> Self {
        Self {
            count: Mutex::new(count),
            condvar: Condvar::new(),
        }
    }

    async fn count_down(&self) {
        let mut count = self.count.lock().await;
        *count -= 1;
        if *count == 0 {
            self.condvar.notify_all();
        }
    }

    async fn wait(&self) {
        let mut count = self.count.lock().await;
        while *count > 0 {
            count = self.condvar.wait(count).await;
        }
    }
}

const QUEUE_SIZE: QueueSize = QueueSize::Limited(10);
const TIME_OUT: Duration = Duration::from_millis(100);

#[tokio::test]
#[serial]
async fn test_constructor1() {
    init_logger();

    let mut q = create_blocking_queue(QueueType::VecDequeue, QUEUE_SIZE).await;
    assert_eq!(q.remaining_capacity().await, QUEUE_SIZE);
    assert_eq!(q.poll().await.unwrap(), None);
}

#[tokio::test]
#[serial]
async fn test_constructor2() {
    init_logger();

    let mut q = create_blocking_queue(QueueType::VecDequeue, QueueSize::Limitless).await;
    assert_eq!(QueueSize::Limitless, q.remaining_capacity().await);
    assert_eq!(q.poll().await.unwrap(), None);
}

#[tokio::test]
#[serial]
async fn test_constructor3() {
    init_logger();

    let elements: Vec<i32> = Vec::with_capacity(QUEUE_SIZE.to_usize());
    let q = create_blocking_queue_with_elements(
        QueueType::VecDequeue,
        QueueSize::Limited(elements.len()),
        elements.clone().into_iter(),
    )
        .await;
    assert_eq!(q.remaining_capacity().await, QueueSize::Limited(elements.len()));
}

// #[tokio::test]
// #[serial]
// async fn test_constructor4() {
//   init_logger();
//
//   let elements = populated_blocking_queue(QueueType::VecDequeue, QUEUE_SIZE);
//   let q = create_blocking_queue_with_elements(
//     QueueType::VecDequeue,
//     elements.len().await,
//     elements.clone().iter(),
//   );
//   assert_eq!(
//     q.remaining_capacity().await,
//     QueueSize::Limited(elements.len().await.to_usize())
//   );
// }

#[tokio::test]
#[serial]
async fn test_iter() {
    init_logger();

    let mut q = populated_blocking_queue(QueueType::VecDequeue, QUEUE_SIZE).await;
    let iter = q.clone().iter();
    tokio::pin!(iter);
    while let Some(e) = iter.next().await {
        log::debug!("e = {:?}", e);
    }
    assert_eq!(q.poll().await.unwrap(), None);
    assert_eq!(q.len().await, QueueSize::Limited(0));
}

#[tokio::test]
#[serial]
async fn test_into_iter() {
    init_logger();

    let mut q = populated_blocking_queue(QueueType::VecDequeue, QUEUE_SIZE).await;
    let iter = q.clone().blocking_iter();
    tokio::pin!(iter);
    while let Some(e) = iter.next().await {
        log::debug!("e = {:?}", e);
        if e == (QUEUE_SIZE.to_usize() - 1) as i32 {
            break;
        }
    }
    assert_eq!(q.poll().await.unwrap(), None);
    assert_eq!(q.len().await, QueueSize::Limited(0));
}

#[tokio::test]
#[serial]
async fn test_blocking_iter() {
    init_logger();

    let mut q = populated_blocking_queue(QueueType::VecDequeue, QUEUE_SIZE).await;
    let mut q_cloned = q.clone();
    let mut counter = q.len().await.to_usize();
    let iter = q.blocking_iter();
    tokio::pin!(iter);
    while let Some(e) = iter.next().await {
        log::debug!("{}: e = {:?}", counter, e);
        if counter == 1 {
            q_cloned.interrupt().await;
        }
        counter -= 1;
        if e == (QUEUE_SIZE.to_usize() - 1) as i32 {
            break;
        }
    }
    assert_eq!(q_cloned.len().await, QueueSize::Limited(0));
}

#[tokio::test]
#[serial]
async fn test_into_blocking_iter() {
    init_logger();

    let mut q = populated_blocking_queue(QueueType::VecDequeue, QUEUE_SIZE).await;
    let mut q_cloned = q.clone();
    let mut counter = q.len().await.to_usize();
    let iter = q.blocking_iter();
    tokio::pin!(iter);
    while let Some(e) = iter.next().await {
        log::debug!("{}: e = {:?}", counter, e);
        if counter == 1 {
            q_cloned.interrupt().await;
        }
        counter -= 1;
        if e == (QUEUE_SIZE.to_usize() - 1) as i32 {
            break;
        }
    }
    assert_eq!(q_cloned.len().await, QueueSize::Limited(0));
}

#[tokio::test]
#[serial]
async fn test_empty_full() {
    init_logger();

    let mut q = create_blocking_queue(QueueType::VecDequeue, QueueSize::Limited(2)).await;
    assert!(q.is_empty().await);
    assert_eq!(q.remaining_capacity().await.to_usize(), 2);
    assert!(q.offer(1).await.is_ok());
    assert!(!q.is_empty().await);
    assert!(q.offer(2).await.is_ok());
    assert!(!q.is_empty().await);
    assert_eq!(q.remaining_capacity().await.to_usize(), 0);
    assert!(q.offer(3).await.is_err());

    assert_eq!(q.poll().await.unwrap(), Some(1));
    assert_eq!(q.poll().await.unwrap(), Some(2));
    assert_eq!(q.poll().await.unwrap(), None);
}

#[tokio::test]
#[serial]
async fn test_remaining_capacity() {
    init_logger();

    let mut q = populated_blocking_queue(QueueType::VecDequeue, QUEUE_SIZE).await;
    for i in 0..QUEUE_SIZE.to_usize() {
        let remaining_capacity = q.remaining_capacity().await.to_usize();
        let len = q.len().await.to_usize();
        assert_eq!(remaining_capacity, i);
        assert_eq!(len + remaining_capacity, QUEUE_SIZE.to_usize());
        assert!(q.take().await.is_ok());
    }
    for i in 0..QUEUE_SIZE.to_usize() {
        let remaining_capacity = q.remaining_capacity().await.to_usize();
        let len = q.len().await.to_usize();
        assert_eq!(remaining_capacity, QUEUE_SIZE.to_usize() - i);
        assert_eq!(len + remaining_capacity, QUEUE_SIZE.to_usize());
        assert!(q.offer(i as i32).await.is_ok());
    }
    for i in 0..QUEUE_SIZE.to_usize() {
        assert_eq!(q.poll().await.unwrap(), Some(i as i32));
    }
    assert_eq!(q.poll().await.unwrap(), None);
}

#[tokio::test]
#[serial]
async fn test_offer() {
    init_logger();

    let mut q = create_blocking_queue(QueueType::VecDequeue, QueueSize::Limited(1)).await;
    assert!(q.offer(0).await.is_ok());
    assert!(q.offer(1).await.is_err());

    assert_eq!(q.poll().await.unwrap(), Some(0));
    assert_eq!(q.poll().await.unwrap(), None);
}

#[tokio::test]
#[serial]
async fn test_offer_all() {
    init_logger();

    let mut q = create_blocking_queue(QueueType::VecDequeue, QueueSize::Limited(10)).await;
    let v = [1, 2, 3, 4, 5];

    assert!(q.offer_all(v).await.is_ok());

    assert_eq!(q.poll().await.unwrap(), Some(1));
    assert_eq!(q.poll().await.unwrap(), Some(2));
    assert_eq!(q.poll().await.unwrap(), Some(3));
    assert_eq!(q.poll().await.unwrap(), Some(4));
    assert_eq!(q.poll().await.unwrap(), Some(5));
    assert_eq!(q.poll().await.unwrap(), None);
}

#[tokio::test]
#[serial]
async fn test_put() {
    init_logger();

    let mut q = create_blocking_queue(QueueType::VecDequeue, QUEUE_SIZE).await;
    for i in 0..QUEUE_SIZE.to_usize() {
        q.put(i as i32).await.unwrap();
        assert!(q.contains(&(i as i32)).await);
    }
}

#[tokio::test]
#[serial]
async fn test_blocking_put() {
    init_logger();

    let mut q = create_blocking_queue(QueueType::VecDequeue, QUEUE_SIZE).await;
    let mut q_cloned = q.clone();

    let please_interrupt = Arc::new(CountDownLatch::new(1));
    let please_interrupt_cloned = please_interrupt.clone();

    let handler1 = tokio::spawn(async move {
        for i in 0..QUEUE_SIZE.to_usize() {
            let _ = q_cloned.put(i as i32).await.unwrap();
        }
        assert_eq!(q_cloned.len().await, QUEUE_SIZE);

        q_cloned.interrupt().await;
        match q_cloned.put(99).await {
            Ok(_) => {
                panic!("put: finish: 99, should not be here");
            }
            Err(err) => {
                log::debug!("put: finish: 99, error = {:?}", err);
                let queue_error = err.downcast_ref::<QueueError<i32>>().unwrap();
                assert_eq!(queue_error, &QueueError::InterruptedError);
            }
        }
        assert!(!q_cloned.is_interrupted().await);

        please_interrupt_cloned.count_down().await;
        match q_cloned.put(99).await {
            Ok(_) => {
                panic!("put: finish: 99, should not be here");
            }
            Err(err) => {
                log::debug!("put: finish: 99, error = {:?}", err);
                let queue_error = err.downcast_ref::<QueueError<i32>>().unwrap();
                assert_eq!(queue_error, &QueueError::InterruptedError);
            }
        }
        assert!(!q_cloned.is_interrupted().await);
    });

    please_interrupt.wait().await;
    assert!(!handler1.is_finished());
    q.interrupt().await;
    handler1.await.unwrap();
}

#[tokio::test]
#[serial]
async fn test_put_with_take() {
    init_logger();

    let capacity = 2;

    let mut q = create_blocking_queue(QueueType::VecDequeue, QueueSize::Limited(capacity)).await;
    let mut q_cloned = q.clone();

    let please_take = Arc::new(CountDownLatch::new(1));
    let please_take_cloned = please_take.clone();
    let please_interrupt = Arc::new(CountDownLatch::new(1));
    let please_interrupt_cloned = please_interrupt.clone();

    let handler1 = tokio::spawn(async move {
        for i in 0..capacity {
            let _ = q_cloned.put(i as i32).await;
        }
        please_take_cloned.count_down().await;
        q_cloned.put(86).await.unwrap();

        please_interrupt_cloned.count_down().await;
        match q_cloned.put(99).await {
            Ok(_) => {
                panic!("put: finish: 99, should not be here");
            }
            Err(err) => {
                log::debug!("put: finish: 99, error = {:?}", err);
                let queue_error = err.downcast_ref::<QueueError<i32>>().unwrap();
                assert_eq!(queue_error, &QueueError::InterruptedError);
            }
        }
    });

    please_take.wait().await;
    assert_eq!(q.take().await.unwrap(), Some(0));

    please_interrupt.wait().await;
    q.interrupt().await;

    handler1.await.unwrap();
}

#[tokio::test]
#[serial]
async fn test_blocking_take() {
    init_logger();

    let mut q = populated_blocking_queue(QueueType::VecDequeue, QUEUE_SIZE).await;
    let mut q_cloned = q.clone();

    let please_interrupt = Arc::new(CountDownLatch::new(1));
    let please_interrupt_cloned = please_interrupt.clone();

    let handler1 = tokio::spawn(async move {
        for _ in 0..QUEUE_SIZE.to_usize() {
            let _ = q_cloned.take().await.unwrap();
        }

        q_cloned.interrupt().await;
        match q_cloned.take().await {
            Ok(_) => {
                panic!("take: finish: should not be here");
            }
            Err(err) => {
                log::debug!("take: finish: error = {:?}", err);
                let queue_error = err.downcast_ref::<QueueError<i32>>().unwrap();
                assert_eq!(queue_error, &QueueError::InterruptedError);
            }
        }
        assert!(!q_cloned.is_interrupted().await);

        please_interrupt_cloned.count_down().await;
        match q_cloned.take().await {
            Ok(_) => {
                panic!("take: finish: should not be here");
            }
            Err(err) => {
                log::debug!("take: finish: error = {:?}", err);
                let queue_error = err.downcast_ref::<QueueError<i32>>().unwrap();
                assert_eq!(queue_error, &QueueError::InterruptedError);
            }
        }
        assert!(!q_cloned.is_interrupted().await);
    });

    please_interrupt.wait().await;
    assert!(!handler1.is_finished());
    q.interrupt().await;
    handler1.await.unwrap();
}

#[tokio::test]
#[serial]
async fn test_poll() {
    init_logger();

    let mut q = populated_blocking_queue(QueueType::VecDequeue, QUEUE_SIZE).await;
    for i in 0..QUEUE_SIZE.to_usize() {
        assert_eq!(q.poll().await.unwrap(), Some(i as i32));
    }
    assert_eq!(q.poll().await.unwrap(), None);
}

async fn populated_blocking_queue(queue_type: QueueType, size: QueueSize) -> BlockingQueue<i32, Queue<i32>> {
    let mut q = create_blocking_queue(queue_type, size.clone()).await;
    q.offer_all((0..size.to_usize() as i32).into_iter()).await.unwrap();
    q
}

async fn create_blocking_queue(queue_type: QueueType, size: QueueSize) -> BlockingQueue<i32, Queue<i32>> {
    let q = create_queue::<i32>(queue_type, size).await.with_blocking();
    q
}

async fn create_blocking_queue_with_elements(
    queue_type: QueueType,
    size: QueueSize,
    elements: impl IntoIterator<Item = i32> + Send,
) -> BlockingQueue<i32, Queue<i32>> {
    let q = create_queue_with_elements::<i32>(queue_type, size, elements)
        .await
        .with_blocking();
    q
}