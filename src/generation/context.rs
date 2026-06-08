use std::collections::HashSet;
use std::rc::Rc;

use crate::ast::Dockerfile;
use crate::ast::Span;

use super::helpers::Node;
use super::helpers::parse_comments;
use crate::configuration::Configuration;

pub struct Context<'a> {
  pub _config: &'a Configuration,
  pub dockerfile: &'a Dockerfile,
  pub text: &'a str,
  pub handled_comments: HashSet<usize>,
  current_node: Option<Node<'a>>,
  parent_stack: Vec<Node<'a>>,
  pub gen_string_content: bool,
  /// whether the current breakable string is shell content whose insignificant
  /// whitespace runs should be collapsed
  pub collapse_shell_ws: bool,
  /// the quote currently open while collapsing shell whitespace (carried across
  /// the breakable string's components), or `None` when outside a quote
  pub shell_quote: Option<char>,
}

impl<'a> Context<'a> {
  pub fn new(text: &'a str, dockerfile: &'a Dockerfile, config: &'a Configuration) -> Self {
    Self {
      _config: config,
      text,
      dockerfile,
      handled_comments: HashSet::new(),
      current_node: None,
      parent_stack: Vec::new(),
      gen_string_content: false,
      collapse_shell_ws: false,
      shell_quote: None,
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
  /// them. When `include_leading` is false, comments appearing before the first
  /// node are skipped so they don't end up on the instruction's prefix line; the
  /// formatter's safety net recovers those onto their own lines instead. The
  /// top-level call uses `true` so file-leading comments are kept.
  pub fn gen_nodes_with_comments(&mut self, start_pos: usize, end_pos: usize, include_leading: bool, nodes: impl Iterator<Item = Node<'a>>) -> Vec<Node<'a>> {
    let mut result = Vec::new();
    let mut last_pos = start_pos;
    let mut is_first = true;
    for node in nodes {
      if !is_first || include_leading {
        let text = &self.text[last_pos..node.span().start];
        for comment in parse_comments(text, last_pos) {
          result.push(Node::CommentRc(Rc::new(comment)));
        }
      }
      let node_end = node.span().end;
      result.push(node);
      last_pos = node_end;
      is_first = false;
    }
    let text = &self.text[last_pos..end_pos];
    for comment in parse_comments(text, last_pos) {
      result.push(Node::CommentRc(Rc::new(comment)));
    }
    result
  }
}
