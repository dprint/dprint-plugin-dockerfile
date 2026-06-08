use std::rc::Rc;

use crate::ast::*;

macro_rules! create_node_ref {
  ($($variant_name:ident($node_name:ident),)*) => {
    #[derive(Clone)]
    pub enum Node<'a> {
      /// our own created comment
      CommentRc(Rc<SpannedComment>),
      $(
        $variant_name(&'a $node_name),
      )*
    }

    $(
      impl<'a> From<&'a $node_name> for Node<'a> {
        fn from(instruction: &'a $node_name) -> Node<'a> {
          Node::$variant_name(instruction)
        }
      }
    )*
  }
}

create_node_ref!(
  Arg(ArgInstruction),
  Cmd(CmdInstruction),
  Copy(CopyInstruction),
  CopyFlag(CopyFlag),
  From(FromInstruction),
  FromFlag(FromFlag),
  Label(LabelInstruction),
  LabelLabel(Label),
  Run(RunInstruction),
  Entrypoint(EntrypointInstruction),
  Env(EnvInstruction),
  EnvVar(EnvVar),
  Shell(ShellInstruction),
  Onbuild(OnbuildInstruction),
  Healthcheck(HealthcheckInstruction),
  Heredoc(HeredocInstruction),
  Misc(MiscInstruction),
  String(SpannedString),
  BreakableString(BreakableString),
  StringArray(StringArray),
  Comment(SpannedComment),
);

impl<'a> Node<'a> {
  #[allow(dead_code)]
  pub fn span(&self) -> Span {
    use Node::*;
    match self {
      From(node) => node.span,
      FromFlag(node) => node.span,
      Arg(node) => node.span,
      Label(node) => node.span,
      LabelLabel(node) => node.span,
      Run(node) => node.span,
      Entrypoint(node) => node.span,
      Cmd(node) => node.span,
      Copy(node) => node.span,
      CopyFlag(node) => node.span,
      Env(node) => node.span,
      EnvVar(node) => node.span,
      Shell(node) => node.span,
      Onbuild(node) => node.span,
      Healthcheck(node) => node.span,
      Heredoc(node) => node.span,
      Misc(node) => node.span,
      String(node) => node.span,
      BreakableString(node) => node.span,
      StringArray(node) => node.span,
      Comment(node) => node.span,
      CommentRc(node) => node.span,
    }
  }

  pub fn is_comment(&self) -> bool {
    matches!(self, Node::Comment(_) | Node::CommentRc(_))
  }
}

pub fn parse_comments(text: &str, offset: usize) -> Vec<SpannedComment> {
  let mut comments = Vec::new();
  let mut char_iterator = text.char_indices();
  let mut in_start_comment_context = true;

  while let Some((i, c)) = char_iterator.next() {
    // leading whitespace is supported but discouraged
    if in_start_comment_context && c.is_whitespace() {
      continue;
    }

    if in_start_comment_context && matches!(c, '#') {
      let start_index = i;
      // include the `#` itself so an empty comment is treated like any other
      let mut end_index = i + c.len_utf8();
      for (i, c) in char_iterator.by_ref() {
        if c == '\n' {
          break;
        }
        end_index = i + c.len_utf8();
      }
      comments.push(SpannedComment {
        span: Span::new(offset + start_index, offset + end_index),
        content: text[start_index..end_index].to_string(),
      });
      in_start_comment_context = true;
    } else {
      in_start_comment_context = matches!(c, '\n');
    }
  }

  comments
}

impl<'a> From<&'a Instruction> for Node<'a> {
  fn from(instruction: &'a Instruction) -> Node<'a> {
    use Instruction::*;
    match instruction {
      From(node) => node.into(),
      Arg(node) => node.into(),
      Label(node) => node.into(),
      Run(node) => node.into(),
      Entrypoint(node) => node.into(),
      Cmd(node) => node.into(),
      Copy(node) => node.into(),
      Env(node) => node.into(),
      Shell(node) => node.into(),
      Onbuild(node) => node.into(),
      Healthcheck(node) => node.into(),
      Heredoc(node) => node.into(),
      Misc(node) => node.into(),
    }
  }
}

impl<'a> From<&'a BreakableStringComponent> for Node<'a> {
  fn from(component: &'a BreakableStringComponent) -> Node<'a> {
    use BreakableStringComponent::*;
    match component {
      String(node) => node.into(),
      Comment(node) => node.into(),
    }
  }
}
