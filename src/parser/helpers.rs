use dockerfile_parser::*;

macro_rules! create_node_ref {
  ($($variant_name:ident($node_name:ident),)*) => {
    #[derive(Clone, Copy)]
    pub enum Node<'a> {
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
  Label(LabelInstruction),
  LabelLabel(Label),
  Run(RunInstruction),
  Entrypoint(EntrypointInstruction),
  Env(EnvInstruction),
  EnvVar(EnvVar),
  Misc(MiscInstruction),
  String(SpannedString),
  BreakableString(BreakableString),
  StringArray(StringArray),
  Comment(Comment),
);

impl<'a> Node<'a> {
  #[allow(dead_code)]
  pub fn span(&self) -> Span {
    use Node::*;
    match self {
      From(node) => node.span,
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
      Misc(node) => node.span,
      String(node) => node.span,
      BreakableString(node) => node.span,
      StringArray(node) => node.span,
      Comment(node) => node.span,
    }
  }
}

pub struct Comment {
  pub span: Span,
  pub text: String,
}

pub fn parse_comments(text: &str, offset: usize) -> Vec<Comment> {
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
      let mut end_index = i;
      while let Some((i, c)) = char_iterator.next() {
        if c == '\n' {
          break;
        }
        end_index = i + c.len_utf8();
      }
      comments.push(Comment {
        span: Span::new(offset + start_index, offset + end_index),
        text: text[start_index + 1..end_index].to_string(),
      });
      in_start_comment_context = true;
    } else {
      in_start_comment_context = false;
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
      Misc(node) => node.into(),
    }
  }
}
