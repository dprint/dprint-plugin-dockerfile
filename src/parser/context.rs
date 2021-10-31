use std::collections::HashSet;
use std::rc::Rc;

use dockerfile_parser::Dockerfile;
use dockerfile_parser::Span;

use super::helpers::parse_comments;
use super::helpers::Node;
use crate::configuration::Configuration;

pub struct Context<'a> {
  pub config: &'a Configuration,
  pub dockerfile: &'a Dockerfile,
  pub text: &'a str,
  pub handled_comments: HashSet<usize>,
  current_node: Option<Node<'a>>,
  parent_stack: Vec<Node<'a>>,
  pub parse_string_content: bool,
}

impl<'a> Context<'a> {
  pub fn new(text: &'a str, dockerfile: &'a Dockerfile, config: &'a Configuration) -> Self {
    Self {
      config,
      text,
      dockerfile,
      handled_comments: HashSet::new(),
      current_node: None,
      parent_stack: Vec::new(),
      parse_string_content: false,
    }
  }

  pub fn span_text(&self, span: &Span) -> &str {
    &self.text[span.start..span.end]
  }

  pub fn set_current_node(&mut self, node: Node<'a>) {
    if let Some(parent) = self.current_node.take() {
      self.parent_stack.push(parent);
    }
    self.current_node = Some(node);
  }

  pub fn pop_current_node(&mut self) {
    self.current_node = self.parent_stack.pop();
  }

  pub fn parent(&self) -> Option<&Node<'a>> {
    self.parent_stack.last()
  }

  pub fn parse_nodes_with_comments(&mut self, start_pos: usize, end_pos: usize, nodes: impl Iterator<Item = Node<'a>>) -> Vec<Node<'a>> {
    let mut result = Vec::new();
    let mut last_pos = start_pos;
    for node in nodes {
      let text = &self.text[last_pos..node.span().start];
      for comment in parse_comments(text, last_pos) {
        result.push(Node::CommentRc(Rc::new(comment)));
      }
      let node_end = node.span().end;
      result.push(node);
      last_pos = node_end;
    }
    let text = &self.text[last_pos..end_pos];
    for comment in parse_comments(text, last_pos) {
      result.push(Node::CommentRc(Rc::new(comment)));
    }
    result
  }
}
