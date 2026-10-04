use std::collections::HashSet;

use crate::ast::Dockerfile;
use crate::ast::Span;

use super::helpers::Node;
use super::helpers::nodes_with_comments;
use crate::configuration::Configuration;

pub struct Context<'a> {
  pub config: &'a Configuration,
  pub dockerfile: &'a Dockerfile<'a>,
  pub text: &'a str,
  pub handled_comments: HashSet<usize>,
  current_node: Option<Node<'a>>,
  parent_stack: Vec<Node<'a>>,
  pub gen_string_content: bool,
  /// Whether the current breakable string is shell content whose insignificant
  /// whitespace runs should be collapsed
  pub collapse_shell_ws: bool,
  /// The quote currently open while collapsing shell whitespace (carried across
  /// the breakable string's components), or `None` when outside a quote
  pub shell_quote: Option<char>,
  /// The amount of leading whitespace to strip from the continuation lines of
  /// the current instruction so the printer can re-indent them
  pub dedent_width: usize,
}

impl<'a> Context<'a> {
  pub fn new(text: &'a str, dockerfile: &'a Dockerfile<'a>, config: &'a Configuration) -> Self {
    Self {
      config,
      text,
      dockerfile,
      handled_comments: HashSet::new(),
      current_node: None,
      parent_stack: Vec::new(),
      gen_string_content: false,
      collapse_shell_ws: false,
      shell_quote: None,
      dedent_width: 0,
    }
  }

  pub fn span_text(&self, span: &Span) -> &'a str {
    &self.text[span.start..span.end]
  }

  /// The line-continuation / escape character for this file (`\` or `` ` ``).
  pub fn escape(&self) -> char {
    self.dockerfile.escape
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

  /// Interleaves the given nodes with the comments found in the text between
  /// them. See [`nodes_with_comments`].
  pub fn gen_nodes_with_comments(&mut self, start_pos: usize, end_pos: usize, include_leading: bool, nodes: impl Iterator<Item = Node<'a>>) -> Vec<Node<'a>> {
    nodes_with_comments(self.text, start_pos, end_pos, include_leading, nodes)
  }
}
