mod queue_linkedlist;
mod queue_mpsc;
pub mod queue_vec;

use crate::core::util::element::Element;
// use crate::core::util::queue::blocking_queue::BlockingQueue;
use crate::core::util::queue::queue_linkedlist::{QueueLinkedList, QueueLinkedListReceiver, QueueLinkedListSender};
use crate::core::util::queue::queue_mpsc::{QueueMPSC, QueueMPSCReceiver, QueueMPSCSender};
use crate::core::util::queue::queue_vec::{QueueVec, QueueVecReceiver, QueueVecSender};
use async_trait::async_trait;
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

/// A trait that defines the behavior of a queue.<br/>
/// キューの振る舞いを定義するトレイト。
#[async_trait::async_trait]
pub trait QueueBehavior<E: Element>: Send + Sized + Sync {
  /// Returns whether this queue is empty.<br/>
  /// このキューが空かどうかを返します。
  ///
  /// # Return Value / 戻り値
  /// - `true` - If the queue is empty. / キューが空の場合。
  /// - `false` - If the queue is not empty. / キューが空でない場合。
  async fn is_empty(&self) -> bool {
    self.len().await == QueueSize::Limited(0)
  }

  /// Returns whether this queue is non-empty.<br/>
  /// このキューが空でないかどうかを返します。
  ///
  /// # Return Value / 戻り値
  /// - `true` - If the queue is not empty. / キューが空でない場合。
  /// - `false` - If the queue is empty. / キューが空の場合。
  async fn non_empty(&self) -> bool {
    !self.is_empty().await
  }

  /// Returns whether the queue size has reached its capacity.<br/>
  /// このキューのサイズが容量まで到達したかどうかを返します。
  ///
  /// # Return Value / 戻り値
  /// - `true` - If the queue size has reached its capacity. / キューのサイズが容量まで到達した場合。
  /// - `false` - If the queue size has not reached its capacity. / キューのサイズが容量まで到達してない場合。
  async fn is_full(&self) -> bool {
    self.capacity().await == self.len().await
  }

  /// Returns whether the queue size has not reached its capacity.<br/>
  /// このキューのサイズが容量まで到達してないかどうかを返します。
  ///
  /// # Return Value / 戻り値
  /// - `true` - If the queue size has not reached its capacity. / キューのサイズが容量まで到達してない場合。
  /// - `false` - If the queue size has reached its capacity. / キューのサイズが容量まで到達した場合。
  async fn non_full(&self) -> bool {
    !self.is_full().await
  }

  /// Returns the length of this queue.<br/>
  /// このキューの長さを返します。
  ///
  /// # Return Value / 戻り値
  /// - `QueueSize::Limitless` - If the queue has no capacity limit. / キューに容量制限がない場合。
  /// - `QueueSize::Limited(num)` - If the queue has a capacity limit. / キューに容量制限がある場合。
  async fn len(&self) -> QueueSize;

  /// Returns the capacity of this queue.<br/>
  /// このキューの最大容量を返します。
  ///
  /// # Return Value / 戻り値
  /// - `QueueSize::Limitless` - If the queue has no capacity limit. / キューに容量制限がない場合。
  /// - `QueueSize::Limited(num)` - If the queue has a capacity limit. / キューに容量制限がある場合。
  async fn capacity(&self) -> QueueSize;
}

#[async_trait::async_trait]
pub trait QueueWriteFactoryBehavior<E: Element>: QueueBehavior<E> {
  type Writer: QueueWriteBehavior<E>;
  fn writer(&self) -> Self::Writer;
}

#[async_trait::async_trait]
pub trait QueueWriteBehavior<E: Element>: QueueBehavior<E> {
  /// The specified element will be inserted into this queue,
  /// if the queue can be executed immediately without violating the capacity limit.<br/>
  /// 容量制限に違反せずにすぐ実行できる場合は、指定された要素をこのキューに挿入します。
  ///
  /// # Arguments / 引数
  /// - `element` - The element to be inserted. / 挿入する要素。
  ///
  /// # Return Value / 戻り値
  /// - `Ok(())` - If the element is inserted successfully. / 要素が正常に挿入された場合。
  /// - `Err(QueueError::OfferError(element))` - If the element cannot be inserted. / 要素を挿入できなかった場合。
  async fn offer(&mut self, element: E) -> Result<(), QueueError<E>>;

  /// The specified elements will be inserted into this queue,
  /// if the queue can be executed immediately without violating the capacity limit.<br/>
  /// 容量制限に違反せずにすぐ実行できる場合は、指定された複数の要素をこのキューに挿入します。
  ///
  /// # Arguments / 引数
  /// - `elements` - The elements to be inserted. / 挿入する要素。
  ///
  /// # Return Value / 戻り値
  /// - `Ok(())` - If the elements are inserted successfully. / 要素が正常に挿入された場合。
  /// - `Err(QueueError::OfferError(element))` - If the elements cannot be inserted. / 要素を挿入できなかった場合。
  async fn offer_all(&mut self, elements: impl IntoIterator<Item = E> + Send) -> Result<(), QueueError<E>> {
    let elements: Vec<E> = elements.into_iter().collect();
    for e in elements {
      self.offer(e).await?;
    }
    Ok(())
  }
}

#[async_trait::async_trait]
pub trait QueueReadFactoryBehavior<E: Element>: QueueBehavior<E> {
  type Reader: QueueReadBehavior<E>;
  fn reader(&self) -> Self::Reader;
}

#[async_trait::async_trait]
pub trait QueueReadBehavior<E: Element>: QueueBehavior<E> {
  /// Retrieves and deletes the head of the queue. Returns None if the queue is empty.<br/>
  /// キューの先頭を取得および削除します。キューが空の場合は None を返します。
  ///
  /// # Return Value / 戻り値
  /// - `Ok(Some(element))` - If the element is retrieved successfully. / 要素が正常に取得された場合。
  /// - `Ok(None)` - If the queue is empty. / キューが空の場合。
  async fn poll(&mut self) -> Result<Option<E>, QueueError<E>>;
}

/// A trait that defines the behavior of a queue that can be peeked.<br/>
/// Peekができるキューの振る舞いを定義するトレイト。
#[async_trait::async_trait]
pub trait HasPeekBehavior<E: Element>: QueueReadBehavior<E> {
  /// Gets the head of the queue, but does not delete it. Returns None if the queue is empty.<br/>
  /// キューの先頭を取得しますが、削除しません。キューが空の場合は None を返します。
  ///
  /// # Return Value / 戻り値
  /// - `Ok(Some(element))` - If the element is retrieved successfully. / 要素が正常に取得された場合。
  /// - `Ok(None)` - If the queue is empty. / キューが空の場合。
  async fn peek(&self) -> Result<Option<E>, QueueError<E>>;
}

/// A trait that defines the behavior of a queue that can be checked for contains.<br/>
/// Containsができるキューの振る舞いを定義するトレイト。
#[async_trait::async_trait]
pub trait HasContainsBehavior<E: Element>: QueueReadBehavior<E> {
  /// Returns whether the specified element is contained in this queue.<br/>
  /// 指定された要素がこのキューに含まれているかどうかを返します。
  ///
  /// # Arguments / 引数
  /// - `element` - The element to be checked. / チェックする要素。
  ///
  /// # Return Value / 戻り値
  /// - `true` - If the element is contained in this queue. / 要素がこのキューに含まれている場合。
  /// - `false` - If the element is not contained in this queue. / 要素がこのキューに含まれていない場合。
  async fn contains(&self, element: &E) -> bool;
}

/// A trait that defines the behavior of a blocking queue.<br/>
/// ブロッキングキューの振る舞いを定義するトレイト。
#[async_trait::async_trait]
pub trait BlockingQueueBehavior<E: Element>: QueueBehavior<E> + Send {
  /// Returns the number of elements that can be inserted into this queue without blocking.<br/>
  /// ブロックせずにこのキューに挿入できる要素数を返します。
  ///
  /// # Return Value / 戻り値
  /// - `QueueSize::Limitless` - If the queue has no capacity limit. / キューに容量制限がない場合。
  /// - `QueueSize::Limited(num)` - If the queue has a capacity limit. / キューに容量制限がある場合。
  async fn remaining_capacity(&self) -> QueueSize;

  /// Returns whether the operation of this queue has been interrupted.<br/>
  /// このキューの操作が中断されたかどうかを返します。
  ///
  /// # Return Value / 戻り値
  /// - `true` - If the operation is interrupted. / 操作が中断された場合。
  /// - `false` - If the operation is not interrupted. / 操作が中断されていない場合。
  async fn is_interrupted(&self) -> bool;
}

#[async_trait::async_trait]
pub trait BlockingQueueWriteBehavior<E: Element>: BlockingQueueBehavior<E> + QueueWriteBehavior<E> {
  /// Inserts the specified element into this queue. If necessary, waits until space is available.<br/>
  /// 指定された要素をこのキューに挿入します。必要に応じて、空きが生じるまで待機します。
  ///
  /// # Arguments / 引数
  /// - `element` - The element to be inserted. / 挿入する要素。
  ///
  /// # Return Value / 戻り値
  /// - `Ok(())` - If the element is inserted successfully. / 要素が正常に挿入された場合。
  /// - `Err(QueueError::OfferError(element))` - If the element cannot be inserted. / 要素を挿入できなかった場合。
  /// - `Err(QueueError::InterruptedError)` - If the operation is interrupted. / 操作が中断された場合。
  async fn put(&mut self, element: E) -> Result<(), QueueError<E>>;

  /// Interrupts the operation of this queue.<br/>
  /// このキューの操作を中断します。
  async fn interrupt(&mut self);
}

#[async_trait::async_trait]
pub trait BlockingQueueReadBehavior<E: Element>: BlockingQueueBehavior<E> {
  /// Retrieve the head of this queue and delete it. If necessary, wait until an element becomes available.<br/>
  /// このキューの先頭を取得して削除します。必要に応じて、要素が利用可能になるまで待機します。
  ///
  /// # Return Value / 戻り値
  /// - `Ok(Some(element))` - If the element is retrieved successfully. / 要素が正常に取得された場合。
  /// - `Ok(None)` - If the queue is empty. / キューが空の場合。
  /// - `Err(QueueError::InterruptedError)` - If the operation is interrupted. / 操作が中断された場合。
  async fn take(&mut self) -> Result<Option<E>, QueueError<E>>;
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

#[derive(Debug, Clone)]
pub enum QueueWriter<T: Element> {
  VecDequeue(QueueVecSender<T>),
  LinkedList(QueueLinkedListSender<T>),
  MPSC(QueueMPSCSender<T>),
}

#[derive(Debug, Clone)]
pub enum QueueReader<T: Element> {
  VecDequeue(QueueVecReceiver<T>),
  LinkedList(QueueLinkedListReceiver<T>),
  MPSC(QueueMPSCReceiver<T>),
}

impl<E: Element + 'static> QueueWriteFactoryBehavior<E> for Queue<E> {
  type Writer = QueueWriter<E>;

  fn writer(&self) -> Self::Writer {
    match self {
      Queue::VecDequeue(inner) => QueueWriter::VecDequeue(inner.writer()),
      Queue::LinkedList(inner) => QueueWriter::LinkedList(inner.writer()),
      Queue::MPSC(inner) => QueueWriter::MPSC(inner.writer()),
    }
  }
}

impl<E: Element + 'static> QueueReadFactoryBehavior<E> for Queue<E> {
  type Reader = QueueReader<E>;

  fn reader(&self) -> Self::Reader {
    match self {
      Queue::VecDequeue(inner) => QueueReader::VecDequeue(inner.reader()),
      Queue::LinkedList(inner) => QueueReader::LinkedList(inner.reader()),
      Queue::MPSC(inner) => QueueReader::MPSC(inner.reader()),
    }
  }
}

#[async_trait::async_trait]
impl<E: Element + 'static> QueueBehavior<E> for QueueWriter<E> {
  async fn len(&self) -> QueueSize {
    match self {
      QueueWriter::VecDequeue(inner) => inner.len().await,
      QueueWriter::LinkedList(inner) => inner.len().await,
      QueueWriter::MPSC(inner) => inner.len().await,
    }
  }

  async fn capacity(&self) -> QueueSize {
    match self {
      QueueWriter::VecDequeue(inner) => inner.capacity().await,
      QueueWriter::LinkedList(inner) => inner.capacity().await,
      QueueWriter::MPSC(inner) => inner.capacity().await,
    }
  }
}

#[async_trait::async_trait]
impl<E: Element + 'static> QueueWriteBehavior<E> for QueueWriter<E> {
  async fn offer(&mut self, element: E) -> Result<(), QueueError<E>> {
    match self {
      QueueWriter::VecDequeue(inner) => inner.offer(element).await,
      QueueWriter::LinkedList(inner) => inner.offer(element).await,
      QueueWriter::MPSC(inner) => inner.offer(element).await,
    }
  }
}

#[async_trait::async_trait]
impl<E: Element + 'static> QueueBehavior<E> for QueueReader<E> {
  async fn len(&self) -> QueueSize {
    match self {
      QueueReader::VecDequeue(inner) => inner.len().await,
      QueueReader::LinkedList(inner) => inner.len().await,
      QueueReader::MPSC(inner) => inner.len().await,
    }
  }

  async fn capacity(&self) -> QueueSize {
    match self {
      QueueReader::VecDequeue(inner) => inner.capacity().await,
      QueueReader::LinkedList(inner) => inner.capacity().await,
      QueueReader::MPSC(inner) => inner.capacity().await,
    }
  }
}

#[async_trait::async_trait]
impl<E: Element + 'static> QueueReadBehavior<E> for QueueReader<E> {
  async fn poll(&mut self) -> Result<Option<E>, QueueError<E>> {
    match self {
      QueueReader::VecDequeue(inner) => inner.poll().await,
      QueueReader::LinkedList(inner) => inner.poll().await,
      QueueReader::MPSC(inner) => inner.poll().await,
    }
  }
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

  // pub fn with_blocking(self) -> BlockingQueue<T, Queue<T>> {
  //   BlockingQueue::new(self)
  // }
}

#[async_trait::async_trait]
impl<T: Element + 'static> QueueBehavior<T> for Queue<T> {
  async fn len(&self) -> QueueSize {
    match self {
      Queue::VecDequeue(inner) => inner.len().await,
      Queue::LinkedList(inner) => inner.len().await,
      Queue::MPSC(inner) => inner.len().await,
    }
  }

  async fn capacity(&self) -> QueueSize {
    match self {
      Queue::VecDequeue(inner) => inner.capacity().await,
      Queue::LinkedList(inner) => inner.capacity().await,
      Queue::MPSC(inner) => inner.capacity().await,
    }
  }
}

pub struct QueueStreamIter<E, QR> {
  q: QR,
  current_future: Option<Pin<Box<dyn Future<Output = Result<Option<E>, QueueError<E>>> + Send>>>,
  p: PhantomData<E>,
}

impl<E: Element + 'static, QR: QueueReadBehavior<E>> QueueStreamIter<E, QR> {
  pub fn new(q: QR) -> Self {
    QueueStreamIter {
      q,
      current_future: None,
      p: PhantomData,
    }
  }
}

impl<E: Element + Unpin + 'static, QR: QueueReadBehavior<E> + Unpin> Stream for QueueStreamIter<E, QR> {
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
