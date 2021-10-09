use std::collections::HashSet;

use super::helpers::Node;
use crate::configuration::Configuration;

pub struct Context<'a> {
  pub config: &'a Configuration,
  pub text: &'a str,
  pub handled_comments: HashSet<usize>,
  pub current_node: Option<Node<'a>>,
}

impl<'a> Context<'a> {
  pub fn new(text: &'a str, config: &'a Configuration) -> Self {
    Self {
      config,
      text,
      handled_comments: HashSet::new(),
      current_node: None,
    }
  }
}
