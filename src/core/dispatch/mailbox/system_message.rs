use std::fmt::{Debug, Formatter};
use crate::core::util::element::Element;

#[derive(Debug, Clone)]
pub enum SystemMessage {
  Create,
  Suspend,
  Resume,
  Terminate,
}

impl Element for SystemMessage {}
