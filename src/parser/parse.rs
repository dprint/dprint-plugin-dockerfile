use std::collections::HashMap;

use dockerfile_parser::*;
use dprint_core::formatting::parser_helpers::parse_raw_string;
use dprint_core::formatting::*;

use super::context::Context;
use super::helpers::*;
use crate::configuration::Configuration;

pub fn parse_items(file: &Dockerfile, text: &str, config: &Configuration) -> PrintItems {
  let top_level_comments = get_top_level_comments(file, text);
  let top_level_nodes = get_top_level_nodes(&top_level_comments, file);

  return parse_items_inner(top_level_nodes, text, config);

  fn get_top_level_comments(file: &Dockerfile, text: &str) -> HashMap<usize, Vec<Comment>> {
    let mut result = HashMap::new();
    let mut last_pos = 0;
    for instruction in file.instructions.iter() {
      let text = &text[last_pos..instruction.span().start];
      result.insert(last_pos, parse_comments(text, last_pos));
      last_pos = instruction.span().end;
    }
    result.insert(last_pos, parse_comments(&text[last_pos..], last_pos));
    result
  }

  fn get_top_level_nodes<'a>(top_level_comments: &'a HashMap<usize, Vec<Comment>>, file: &'a Dockerfile) -> Vec<Node<'a>> {
    let mut result = Vec::new();
    let mut last_pos = 0;
    for instruction in file.instructions.iter() {
      if let Some(comments) = top_level_comments.get(&last_pos) {
        for comment in comments.iter() {
          result.push(comment.into());
        }
      }
      result.push(instruction.into());
      last_pos = instruction.span().end;
    }
    if let Some(comments) = top_level_comments.get(&last_pos) {
      for comment in comments.iter() {
        result.push(comment.into());
      }
    }
    result
  }
}

pub fn parse_items_inner(top_level_nodes: Vec<Node>, text: &str, config: &Configuration) -> PrintItems {
  let mut context = Context::new(text, config);
  let mut items = PrintItems::new();

  for (i, node) in top_level_nodes.iter().enumerate() {
    items.extend(parse_node(*node, &mut context));
    items.push_signal(Signal::NewLine);
    if let Some(next_node) = top_level_nodes.get(i + 1) {
      let text_between = &text[node.span().end..next_node.span().start];
      if text_between.chars().filter(|c| *c == '\n').count() > 1 {
        items.push_signal(Signal::NewLine);
      }
    }
  }

  /*
  items.push_condition(if_true(
    "endOfFileNewLine",
    |context| Some(context.writer_info.column_number > 0),
    Signal::NewLine.into(),
  ));*/

  items
}

fn parse_node<'a>(node: Node<'a>, context: &mut Context<'a>) -> PrintItems {
  let mut items = PrintItems::new();

  // todo: remove?
  /*
  let previous_node_end = context.current_node.as_ref().map(|n| n.span().end).unwrap_or(0);
  if previous_node_end < node.span().start {
    let previous_text = &context.text[previous_node_end..node.span().start];
    let unhandled_comments = parse_comments(previous_text, previous_node_end)
      .into_iter()
      .filter(|c| !context.handled_comments.contains(&c.span.start))
      .collect::<Vec<_>>();
    let mut previous_end = previous_node_end;
    for comment in unhandled_comments {
      let text_between = &context.text[previous_end..comment.span.start];
      if text_between.chars().filter(|c| *c == '\n').count() > 1 {
        items.push_signal(Signal::NewLine);
      }
      previous_end = comment.span.end
    }
  }*/

  context.current_node = Some(node);
  items.extend(match node {
    Node::Arg(node) => parse_arg_instruction(node, context),
    Node::Cmd(node) => parse_cmd_instruction(node, context),
    Node::Copy(node) => parse_copy_instruction(node, context),
    Node::Entrypoint(node) => parse_entrypoint_instruction(node, context),
    Node::Env(node) => parse_env_instruction(node, context),
    Node::EnvVar(node) => parse_env_var(node, context),
    Node::From(node) => parse_from_instruction(node, context),
    Node::Label(node) => parse_label_instruction(node, context),
    Node::LabelLabel(node) => parse_label(node, context),
    Node::Misc(node) => parse_misc_instruction(node, context),
    Node::Run(node) => parse_run_instruction(node, context),
    Node::StringArray(node) => parse_string_array(node, context),
    Node::String(node) => parse_as_is(&node.span, context),
    Node::BreakableString(node) => parse_as_is(&node.span, context),
    Node::CopyFlag(node) => parse_as_is(&node.span, context),
    Node::Comment(node) => parse_comment(node, context),
  });
  context.current_node = Some(node);
  items
}

fn parse_arg_instruction<'a>(node: &'a ArgInstruction, context: &mut Context<'a>) -> PrintItems {
  let mut items = PrintItems::new();

  items.push_str("ARG ");
  items.extend(parse_node((&node.name).into(), context));

  if let Some(value) = &node.value {
    items.push_str("=");
    items.extend(parse_node(value.into(), context));
  }

  items
}

fn parse_cmd_instruction<'a>(node: &'a CmdInstruction, context: &mut Context<'a>) -> PrintItems {
  let mut items = PrintItems::new();
  items.push_str("CMD ");
  items.extend(match &node.expr {
    ShellOrExecExpr::Exec(node) => parse_node(node.into(), context),
    ShellOrExecExpr::Shell(node) => parse_node(node.into(), context),
  });
  items
}

fn parse_copy_instruction<'a>(node: &'a CopyInstruction, context: &mut Context<'a>) -> PrintItems {
  let mut items = PrintItems::new();
  items.push_str("COPY ");
  for flag in node.flags.iter() {
    items.extend(parse_node(flag.into(), context));
    items.push_str(" ");
  }
  for source in node.sources.iter() {
    items.extend(parse_node(source.into(), context));
    items.push_str(" ");
  }
  items.extend(parse_node((&node.destination).into(), context));
  items
}

fn parse_entrypoint_instruction<'a>(node: &'a EntrypointInstruction, context: &mut Context<'a>) -> PrintItems {
  let mut items = PrintItems::new();
  items.push_str("ENTRYPOINT ");
  items.extend(match &node.expr {
    ShellOrExecExpr::Exec(node) => parse_node(node.into(), context),
    ShellOrExecExpr::Shell(node) => parse_node(node.into(), context),
  });
  items
}

fn parse_env_instruction<'a>(node: &'a EnvInstruction, context: &mut Context<'a>) -> PrintItems {
  let mut items = PrintItems::new();
  let prefix_str = "ENV ";
  items.push_str(prefix_str);
  items.extend(parse_multi_line_items(
    node.vars.iter().map(|v| v.into()).collect(),
    prefix_str.chars().count() as u32,
    context,
  ));
  items
}

fn parse_env_var<'a>(node: &'a EnvVar, context: &mut Context<'a>) -> PrintItems {
  let mut items = PrintItems::new();
  items.extend(parse_node((&node.key).into(), context));
  items.push_str("=");
  items.extend(parse_node((&node.value).into(), context));
  items
}

fn parse_from_instruction<'a>(node: &'a FromInstruction, context: &mut Context<'a>) -> PrintItems {
  // todo: handle --platform flag https://github.com/HewlettPackard/dockerfile-parser-rs/issues/18
  let mut items = PrintItems::new();
  items.push_str("FROM ");
  items.extend(parse_node((&node.image).into(), context));
  if let Some(alias) = &node.alias {
    items.push_str(" AS ");
    items.extend(parse_node(alias.into(), context));
  }
  items
}

fn parse_label_instruction<'a>(node: &'a LabelInstruction, context: &mut Context<'a>) -> PrintItems {
  let mut items = PrintItems::new();
  let prefix_str = "LABEL ";
  items.push_str(prefix_str);
  items.extend(parse_multi_line_items(
    node.labels.iter().map(|l| l.into()).collect(),
    prefix_str.chars().count() as u32,
    context,
  ));
  items
}

fn parse_label<'a>(node: &'a Label, context: &mut Context<'a>) -> PrintItems {
  let mut items = PrintItems::new();
  items.extend(parse_node((&node.name).into(), context));
  items.push_str("=");
  items.extend(parse_node((&node.value).into(), context));
  items
}

fn parse_multi_line_items<'a>(nodes: Vec<Node<'a>>, indent_width: u32, context: &mut Context<'a>) -> PrintItems {
  let mut items = PrintItems::new();
  let count = nodes.len();
  for (i, node) in nodes.into_iter().enumerate() {
    let mut node_items = parse_node(node, context);
    if i < count - 1 {
      node_items.push_str(" \\");
      node_items.push_signal(Signal::NewLine);
    }

    if i > 0 {
      items.extend(parser_helpers::with_indent_times(node_items, indent_width));
    } else {
      items.extend(node_items);
    }
  }
  items
}

fn parse_misc_instruction<'a>(node: &'a MiscInstruction, context: &mut Context<'a>) -> PrintItems {
  let mut items = PrintItems::new();
  items.extend(parse_node((&node.instruction).into(), context));
  items.push_str(" ");
  items.extend(parse_node((&node.arguments).into(), context));
  items
}

fn parse_run_instruction<'a>(node: &'a RunInstruction, context: &mut Context<'a>) -> PrintItems {
  let mut items = PrintItems::new();

  items.push_str("RUN ");
  items.extend(match &node.expr {
    ShellOrExecExpr::Exec(node) => parse_node(node.into(), context),
    ShellOrExecExpr::Shell(node) => parse_node(node.into(), context),
  });

  items
}

fn parse_string_array<'a>(node: &'a StringArray, context: &mut Context<'a>) -> PrintItems {
  let mut items = PrintItems::new();
  items.push_str("[");
  for (i, element) in node.elements.iter().enumerate() {
    items.extend(parse_node(element.into(), context));
    if i < node.elements.len() - 1 {
      items.push_str(", ");
    }
  }
  items.push_str("]");
  items
}

fn parse_as_is(span: &Span, context: &mut Context) -> PrintItems {
  let mut items = PrintItems::new();
  let text = &context.text[span.start..span.end].trim();
  for (i, line) in text.lines().enumerate() {
    if i > 0 {
      items.push_signal(Signal::NewLine);
    }
    // be strict here, it must start with #
    if line.starts_with("#") {
      items.extend(parse_comment_text(&line[1..]));
    } else {
      items.extend(parse_raw_string(line.trim_end()));
    }
  }
  items
}

fn parse_comment<'a>(comment: &'a Comment, context: &mut Context<'a>) -> PrintItems {
  let mut items = PrintItems::new();
  if !context.handled_comments.insert(comment.span.start) {
    return items;
  }

  items.extend(parse_comment_text(&comment.text));
  items.push_signal(Signal::ExpectNewLine);

  items
}

fn parse_comment_text(text: &str) -> PrintItems {
  let text_start = text.char_indices().skip_while(|(_, c)| *c == '#').next().map(|(index, _)| index).unwrap_or(0);
  format!("#{} {}", &text[..text_start], &text[text_start..].trim()).into()
}
