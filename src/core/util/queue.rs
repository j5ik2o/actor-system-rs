mod blocking_queue;
#[cfg(test)]
mod blocking_queue_test;
mod queue_linkedlist;
mod queue_mpsc;
pub mod queue_vec;

use crate::core::util::element::Element;
use crate::core::util::queue::blocking_queue::BlockingQueue;
use crate::core::util::queue::queue_linkedlist::QueueLinkedList;
use crate::core::util::queue::queue_mpsc::QueueMPSC;
use crate::core::util::queue::queue_vec::QueueVec;
use futures::Stream;
use std::cmp::Ordering;
use std::future::Future;
use std::marker::PhantomData;
use std::pin::Pin;
use std::task::{Context, Poll};
use thiserror::Error;

/// An error that occurs when a queue operation fails.<br/>
/// キューの操作に失敗した場合に発生するエラー。
#[derive(Error, Debug, PartialEq)]
pub enum QueueError<E> {
  #[error("Failed to offer an element: {0:?}")]
  OfferError(E),
  #[error("Failed to pool an element")]
  PoolError,
  #[error("Failed to peek an element")]
  PeekError,
  #[error("Failed to contains an element")]
  ContainsError,
  #[error("Failed to interrupt")]
  InterruptedError,
  #[error("Failed to timeout")]
  TimeoutError,
}

/// The size of the queue.<br/>
/// キューのサイズ。
#[derive(Debug, Clone)]
pub enum QueueSize {
  /// The queue has no capacity limit.<br/>
  /// キューに容量制限がない。
  Limitless,
  /// The queue has a capacity limit.<br/>
  /// キューに容量制限がある。
  Limited(usize),
}

impl QueueSize {
  fn increment(&mut self) {
    match self {
      QueueSize::Limited(c) => {
        *c += 1;
      }
      _ => {}
    }
  }

  fn decrement(&mut self) {
    match self {
      QueueSize::Limited(c) => {
        *c -= 1;
      }
      _ => {}
    }
  }

  /// Returns whether the queue has no capacity limit.<br/>
  /// キューに容量制限がないかどうかを返します。
  ///
  /// # Return Value / 戻り値
  /// - `true` - If the queue has no capacity limit. / キューに容量制限がない場合。
  /// - `false` - If the queue has a capacity limit. / キューに容量制限がある場合。
  pub fn is_limitless(&self) -> bool {
    match self {
      QueueSize::Limitless => true,
      _ => false,
    }
  }

  /// Converts to an option type.<br/>
  /// オプション型に変換します。
  ///
  /// # Return Value / 戻り値
  /// - `None` - If the queue has no capacity limit. / キューに容量制限がない場合。
  /// - `Some(num)` - If the queue has a capacity limit. / キューに容量制限がある場合。
  pub fn to_option(&self) -> Option<usize> {
    match self {
      QueueSize::Limitless => None,
      QueueSize::Limited(c) => Some(*c),
    }
  }

  /// Converts to a usize type.<br/>
  /// usize型に変換します。
  ///
  /// # Return Value / 戻り値
  /// - `usize::MAX` - If the queue has no capacity limit. / キューに容量制限がない場合。
  /// - `num` - If the queue has a capacity limit. / キューに容量制限がある場合。
  pub fn to_usize(&self) -> usize {
    match self {
      QueueSize::Limitless => usize::MAX,
      QueueSize::Limited(c) => *c,
    }
  }
}

impl PartialEq<Self> for QueueSize {
  fn eq(&self, other: &Self) -> bool {
    match (self, other) {
      (QueueSize::Limitless, QueueSize::Limitless) => true,
      (QueueSize::Limited(l), QueueSize::Limited(r)) => l == r,
      _ => false,
    }
  }
}

impl PartialOrd<QueueSize> for QueueSize {
  fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
    match (self, other) {
      (QueueSize::Limitless, QueueSize::Limitless) => Some(Ordering::Equal),
      (QueueSize::Limitless, _) => Some(Ordering::Greater),
      (_, QueueSize::Limitless) => Some(Ordering::Less),
      (QueueSize::Limited(l), QueueSize::Limited(r)) => l.partial_cmp(r),
    }
  }
}

/// A trait that defines the behavior of a queue writer.<br/>
/// キューへの書き込みを定義するトレイト。
#[async_trait::async_trait]
pub trait QueueWriter<E: Element>: Send + Sized + Sync {
  async fn offer(&mut self, element: E) -> Result<(), QueueError<E>>;
  async fn offer_all(&mut self, elements: impl IntoIterator<Item = E> + Send) -> Result<(), QueueError<E>>;
}

/// A trait that defines the behavior of a queue reader.<br/>
/// キューからの読み込みを定義するトレイト。
#[async_trait::async_trait]
pub trait QueueReader<E: Element>: Send + Sized + Sync {
  async fn poll(&mut self) -> Result<Option<E>, QueueError<E>>;
}

/// A trait that defines the behavior of a queue that can be peeked.<br/>
/// Peekができるキューの振る舞いを定義するトレイト。
#[async_trait::async_trait]
pub trait HasPeekBehavior<E: Element>: QueueReader<E> {
  async fn peek(&self) -> Result<Option<E>, QueueError<E>>;
}

/// A trait that defines the behavior of a queue that can be checked for contains.<br/>
/// Containsができるキューの振る舞いを定義するトレイト。
#[async_trait::async_trait]
pub trait HasContainsBehavior<E: Element>: QueueReader<E> {
  async fn contains(&self, element: &E) -> bool;
}

/// A trait that defines the behavior of a blocking queue.<br/>
/// ブロッキングキューの振る舞いを定義するトレイト。
#[async_trait::async_trait]
pub trait BlockingQueueBehavior<E: Element>: QueueWriter<E> + QueueReader<E> + Send {
  async fn put(&mut self, element: E) -> Result<(), QueueError<E>>;
  async fn take(&mut self) -> Result<Option<E>, QueueError<E>>;
  async fn remaining_capacity(&self) -> QueueSize;
  async fn interrupt(&mut self);
  async fn is_interrupted(&self) -> bool;
}

/// An enumeration type that represents the type of queue.<br/>
/// キューの種類を表す列挙型。
pub enum QueueType {
  /// A queue implemented with VecDequeue.<br/>
  /// VecDequeueで実装されたキュー。
  VecDequeue,
  /// A queue implemented with LinkedList.<br/>
  /// LinkedListで実装されたキュー。
  LinkedList,
  /// A queue implemented with MPSC.<br/>
  /// MPSCで実装されたキュー。
  MPSC,
}

/// An enumeration type that represents a queue.<br/>
/// キューを表す列挙型。
#[derive(Debug, Clone)]
pub enum Queue<T: Element> {
  /// A queue implemented with a vector.<br/>
  /// ベクタで実装されたキュー。
  VecDequeue(QueueVec<T>),
  /// A queue implemented with LinkedList.<br/>
  /// LinkedListで実装されたキュー。
  LinkedList(QueueLinkedList<T>),
  /// A queue implemented with MPSC.<br/>
  /// MPSCで実装されたキュー。
  MPSC(QueueMPSC<T>),
}

impl<T: Element + 'static> Queue<T> {
  pub fn as_vec_dequeue_mut(&mut self) -> Option<&mut QueueVec<T>> {
    match self {
      Queue::VecDequeue(inner) => Some(inner),
      _ => None,
    }
  }

  pub fn as_vec_dequeue(&self) -> Option<&QueueVec<T>> {
    match self {
      Queue::VecDequeue(inner) => Some(inner),
      _ => None,
    }
  }

  pub fn as_linked_list_mut(&mut self) -> Option<&mut QueueLinkedList<T>> {
    match self {
      Queue::LinkedList(inner) => Some(inner),
      _ => None,
    }
  }

  pub fn as_linked_list(&self) -> Option<&QueueLinkedList<T>> {
    match self {
      Queue::LinkedList(inner) => Some(inner),
      _ => None,
    }
  }

  pub fn as_mpsc_mut(&mut self) -> Option<&mut QueueMPSC<T>> {
    match self {
      Queue::MPSC(inner) => Some(inner),
      _ => None,
    }
  }

  pub fn as_mpsc(&self) -> Option<&QueueMPSC<T>> {
    match self {
      Queue::MPSC(inner) => Some(inner),
      _ => None,
    }
  }

  /// Converts the queue to a blocking queue.<br/>
  /// キューをブロッキングキューに変換します。
  ///
  /// # Return Value / 戻り値
  /// - `BlockingQueue<T, Queue<T>>` - A blocking queue. / ブロッキングキュー。
  pub fn with_blocking(self) -> BlockingQueue<T, Queue<T>> {
    BlockingQueue::new(self)
  }
}

pub struct QueueStreamIter<E, Q> {
  q: Q,
  current_future: Option<Pin<Box<dyn Future<Output = Result<Option<E>, QueueError<E>>> + Send>>>,
  p: PhantomData<E>,
}

impl<E: Element + 'static, Q: QueueReader<E>> QueueStreamIter<E, Q> {
  pub fn new(q: Q) -> Self {
    QueueStreamIter {
      q,
      current_future: None,
      p: PhantomData,
    }
  }
}

impl<E: Element + Unpin + 'static, Q: QueueReader<E> + Unpin> Stream for QueueStreamIter<E, Q> {
  type Item = E;

  fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
    let bq = &mut self.get_mut().q;
    match Pin::new(&mut bq.poll()).poll(cx) {
      Poll::Ready(Ok(Some(value))) => Poll::Ready(Some(value)),
      Poll::Ready(Ok(None)) => Poll::Ready(None),
      Poll::Pending => Poll::Pending,
      _ => {
        panic!("BlockingQueueIter: poll_next: unexpected error");
      }
    }
  }
}

pub async fn create_queue<T: Element + 'static>(queue_type: QueueType, queue_size: QueueSize) -> Queue<T> {
  match (queue_type, &queue_size) {
    (QueueType::VecDequeue, QueueSize::Limitless) => Queue::VecDequeue(QueueVec::<T>::new()),
    (QueueType::VecDequeue, QueueSize::Limited(_)) => Queue::VecDequeue(QueueVec::<T>::new().with_capacity(queue_size)),
    (QueueType::LinkedList, QueueSize::Limitless) => Queue::LinkedList(QueueLinkedList::<T>::new()),
    (QueueType::LinkedList, QueueSize::Limited(_)) => {
      Queue::LinkedList(QueueLinkedList::<T>::new().with_capacity(queue_size))
    }
    (QueueType::MPSC, QueueSize::Limitless) => Queue::MPSC(QueueMPSC::<T>::new(queue_size.to_usize())),
    (QueueType::MPSC, QueueSize::Limited(_)) => Queue::MPSC(
      QueueMPSC::<T>::new(queue_size.to_usize())
        .with_capacity(queue_size)
        .await,
    ),
  }
}

pub async fn create_queue_with_elements<T: Element + 'static>(
  queue_type: QueueType,
  queue_size: QueueSize,
  values: impl IntoIterator<Item = T> + Send,
) -> Queue<T> {
  let vec = values.into_iter().collect::<Vec<_>>();
  match queue_type {
    QueueType::VecDequeue => Queue::VecDequeue(QueueVec::<T>::new().with_capacity(queue_size).with_elements(vec)),
    QueueType::LinkedList => {
      Queue::LinkedList(QueueLinkedList::<T>::new().with_capacity(queue_size).with_elements(vec))
    }
    QueueType::MPSC => Queue::MPSC(
      QueueMPSC::<T>::new(vec.len())
        .with_capacity(queue_size)
        .await
        .with_elements(vec)
        .await,
    ),
  }
}
