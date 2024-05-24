use crate::core::dispatch::element::Element;

pub trait Message: Element + 'static {}
