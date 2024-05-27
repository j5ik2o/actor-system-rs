use std::fmt::Debug;

use crate::core::util::element::Element;

#[derive(Debug, Clone)]
pub enum SystemMessage {
  Create,
  Suspend,
  Resume,
  Terminate,
}

impl Element for SystemMessage {}
