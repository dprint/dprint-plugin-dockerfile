use dockerfile_parser::*;
use dprint_core::formatting::ir_helpers::gen_from_raw_string;
use dprint_core::formatting::*;

use super::context::Context;
use super::helpers::*;
use crate::configuration::Configuration;

pub fn generate(file: &Dockerfile, text: &str, config: &Configuration) -> PrintItems {
  let mut context = Context::new(text, file, config);
  let mut items = PrintItems::new();
  let top_level_nodes = context.gen_nodes_with_comments(0, text.len(), file.instructions.iter().map(|i| i.into()));

  for (i, node) in top_level_nodes.iter().enumerate() {
    items.extend(gen_node(node.clone(), &mut context));
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

fn gen_node<'a>(node: Node<'a>, context: &mut Context<'a>) -> PrintItems {
  let mut items = PrintItems::new();

  context.set_current_node(node.clone());
  items.extend(match node {
    Node::Arg(node) => gen_arg_instruction(node, context),
    Node::Cmd(node) => gen_cmd_instruction(node, context),
    Node::Copy(node) => gen_copy_instruction(node, context),
    Node::Entrypoint(node) => gen_entrypoint_instruction(node, context),
    Node::Env(node) => gen_env_instruction(node, context),
    Node::EnvVar(node) => gen_env_var(node, context),
    Node::From(node) => gen_from_instruction(node, context),
    Node::Label(node) => gen_label_instruction(node, context),
    Node::LabelLabel(node) => gen_label(node, context),
    Node::Misc(node) => gen_misc_instruction(node, context),
    Node::Run(node) => gen_run_instruction(node, context),
    Node::StringArray(node) => gen_string_array(node, context),
    Node::String(node) => gen_string(node, context),
    Node::BreakableString(node) => gen_breakable_string(node, context),
    Node::CopyFlag(node) => gen_copy_flag(node, context),
    Node::CommentRc(node) => gen_comment(&node, context),
    Node::Comment(node) => gen_comment(node, context),
  });
  context.pop_current_node();
  items
}

fn gen_arg_instruction<'a>(node: &'a ArgInstruction, context: &mut Context<'a>) -> PrintItems {
  let mut items = PrintItems::new();

  items.push_str("ARG ");
  items.extend(gen_node((&node.name).into(), context));

  if let Some(value) = &node.value {
    items.push_str("=");
    items.extend(gen_node(value.into(), context));
  }

  items
}

fn gen_cmd_instruction<'a>(node: &'a CmdInstruction, context: &mut Context<'a>) -> PrintItems {
  let mut items = PrintItems::new();
  items.push_str("CMD ");
  items.extend(match &node.expr {
    ShellOrExecExpr::Exec(node) => gen_node(node.into(), context),
    ShellOrExecExpr::Shell(node) => gen_node(node.into(), context),
  });
  items
}

fn gen_copy_instruction<'a>(node: &'a CopyInstruction, context: &mut Context<'a>) -> PrintItems {
  let mut items = PrintItems::new();
  items.push_str("COPY ");
  for flag in node.flags.iter() {
    items.extend(gen_node(flag.into(), context));
    items.push_str(" ");
  }
  for source in node.sources.iter() {
    items.extend(gen_node(source.into(), context));
    items.push_str(" ");
  }
  items.extend(gen_node((&node.destination).into(), context));
  items
}

fn gen_entrypoint_instruction<'a>(node: &'a EntrypointInstruction, context: &mut Context<'a>) -> PrintItems {
  let mut items = PrintItems::new();
  items.push_str("ENTRYPOINT ");
  items.extend(match &node.expr {
    ShellOrExecExpr::Exec(node) => gen_node(node.into(), context),
    ShellOrExecExpr::Shell(node) => gen_node(node.into(), context),
  });
  items
}

fn gen_env_instruction<'a>(node: &'a EnvInstruction, context: &mut Context<'a>) -> PrintItems {
  let mut items = PrintItems::new();
  let nodes = context.gen_nodes_with_comments(node.span.start, node.span.end, node.vars.iter().map(|i| i.into()));
  let prefix_str = "ENV ";
  items.push_str(prefix_str);
  items.extend(gen_multi_line_items(nodes, prefix_str.chars().count() as u32, context));
  items
}

fn gen_env_var<'a>(node: &'a EnvVar, context: &mut Context<'a>) -> PrintItems {
  let mut items = PrintItems::new();
  items.extend(gen_node((&node.key).into(), context));
  items.push_str("=");
  items.extend(gen_node((&node.value).into(), context));
  items
}

fn gen_from_instruction<'a>(node: &'a FromInstruction, context: &mut Context<'a>) -> PrintItems {
  // todo: handle --platform flag https://github.com/HewlettPackard/dockerfile-parser-rs/issues/18
  let mut items = PrintItems::new();
  items.push_str("FROM ");
  items.extend(gen_node((&node.image).into(), context));
  if let Some(alias) = &node.alias {
    items.push_str(" AS ");
    items.extend(gen_node(alias.into(), context));
  }
  items
}

fn gen_label_instruction<'a>(node: &'a LabelInstruction, context: &mut Context<'a>) -> PrintItems {
  let mut items = PrintItems::new();
  let prefix_str = "LABEL ";
  items.push_str(prefix_str);
  items.extend(gen_multi_line_items(
    node.labels.iter().map(|l| l.into()).collect(),
    prefix_str.chars().count() as u32,
    context,
  ));
  items
}

fn gen_label<'a>(node: &'a Label, context: &mut Context<'a>) -> PrintItems {
  let mut items = PrintItems::new();
  items.extend(gen_node((&node.name).into(), context));
  items.push_str("=");
  items.extend(gen_node((&node.value).into(), context));
  items
}

fn gen_multi_line_items<'a>(nodes: Vec<Node<'a>>, indent_width: u32, context: &mut Context<'a>) -> PrintItems {
  let count = nodes.len();
  let nodes_with_line_index = nodes
    .into_iter()
    .map(|node| {
      let (line_index, _) = node.span().relative_span(context.dockerfile);
      (node, line_index)
    })
    .collect::<Vec<_>>();
  let force_use_new_lines = nodes_with_line_index.len() > 1 && nodes_with_line_index[0].1 < nodes_with_line_index[1].1;

  ir_helpers::gen_separated_values(
    |is_multiline| {
      nodes_with_line_index
        .into_iter()
        .enumerate()
        .map(|(i, (node, line_index))| {
          let is_comment = node.is_comment();
          let mut node_items = gen_node(node, context);
          if i < count - 1 && !is_comment {
            node_items.push_condition(conditions::if_true("endLineText", is_multiline.create_resolver(), {
              let mut items = PrintItems::new();
              items.push_str(" \\");
              items
            }));
          }

          ir_helpers::GeneratedValue {
            items: if i > 0 {
              ir_helpers::with_indent_times(node_items, indent_width)
            } else {
              node_items
            },
            lines_span: Some(ir_helpers::LinesSpan {
              start_line: line_index,
              end_line: line_index,
            }),
            allow_inline_multi_line: false,
            allow_inline_single_line: false,
          }
        })
        .collect()
    },
    ir_helpers::GenSeparatedValuesOptions {
      prefer_hanging: false,
      force_use_new_lines,
      allow_blank_lines: false,
      single_line_space_at_start: false,
      single_line_space_at_end: false,
      single_line_separator: Signal::SpaceOrNewLine.into(),
      indent_width: 0 as u8,
      multi_line_options: ir_helpers::MultiLineOptions::same_line_no_indent(),
      force_possible_newline_at_start: false,
    },
  )
  .items
}

fn gen_misc_instruction<'a>(node: &'a MiscInstruction, context: &mut Context<'a>) -> PrintItems {
  let mut items = PrintItems::new();
  items.extend(gen_node((&node.instruction).into(), context));
  items.push_str(" ");
  items.extend(gen_node((&node.arguments).into(), context));
  items
}

fn gen_run_instruction<'a>(node: &'a RunInstruction, context: &mut Context<'a>) -> PrintItems {
  let mut items = PrintItems::new();

  items.push_str("RUN ");
  items.extend(match &node.expr {
    ShellOrExecExpr::Exec(node) => gen_node(node.into(), context),
    ShellOrExecExpr::Shell(node) => gen_node(node.into(), context),
  });

  items
}

fn gen_string_array<'a>(node: &'a StringArray, context: &mut Context<'a>) -> PrintItems {
  let mut items = PrintItems::new();
  items.push_str("[");
  for (i, element) in node.elements.iter().enumerate() {
    items.extend(gen_node(element.into(), context));
    if i < node.elements.len() - 1 {
      items.push_str(", ");
    }
  }
  items.push_str("]");
  items
}

fn gen_breakable_string<'a>(node: &'a BreakableString, context: &mut Context<'a>) -> PrintItems {
  let mut items = PrintItems::new();
  let is_parent_env_var = matches!(context.parent(), Some(Node::EnvVar(_)));
  let is_quoted = context.span_text(&node.span).starts_with("\"");
  let use_quotes = is_quoted || is_parent_env_var && context.span_text(&node.span).contains(' ');
  let previous_gen_string_content = context.gen_string_content;
  context.gen_string_content = use_quotes;

  if use_quotes {
    items.push_str("\"");
  }
  for (i, component) in node.components.iter().enumerate() {
    items.extend(gen_node(component.into(), context));
    if i < node.components.len() - 1 {
      if let BreakableStringComponent::String(text) = component {
        if !use_quotes && text.content.ends_with(" ") {
          items.push_str(" \\");
        } else {
          items.push_str("\\");
        }
      }
      items.push_signal(Signal::NewLine);
    }
  }
  if use_quotes {
    items.push_str("\"");
  }

  context.gen_string_content = previous_gen_string_content;
  items
}

fn gen_string<'a>(node: &'a SpannedString, context: &mut Context<'a>) -> PrintItems {
  let mut items = PrintItems::new();
  let text = if context.gen_string_content {
    // don't trim this because it's the content
    &node.content
  } else {
    let should_trim = if let Some(Node::BreakableString(parent)) = context.parent() {
      if let Some(BreakableStringComponent::String(str)) = parent.components.first() {
        str.span == node.span
      } else {
        true
      }
    } else {
      true
    };
    let text = context.span_text(&node.span);
    if should_trim {
      text.trim()
    } else {
      text.trim_end()
    }
  };
  items.extend(gen_from_raw_string(text));
  items
}

fn gen_copy_flag<'a>(node: &'a CopyFlag, context: &mut Context<'a>) -> PrintItems {
  // ex: --from=foo
  let mut items = PrintItems::new();
  items.push_str("--");
  items.extend(gen_node((&node.name).into(), context));
  items.push_str("=");
  items.extend(gen_node((&node.value).into(), context));
  items
}

fn gen_comment<'a>(comment: &SpannedComment, context: &mut Context<'a>) -> PrintItems {
  let mut items = PrintItems::new();
  if !context.handled_comments.insert(comment.span.start) {
    return items;
  }

  items.extend(gen_comment_text(&comment.content));
  items.push_signal(Signal::ExpectNewLine);

  items
}

fn gen_comment_text(text: &str) -> PrintItems {
  let text_start = text.char_indices().skip_while(|(_, c)| *c == '#').next().map(|(index, _)| index).unwrap_or(0);
  format!("#{} {}", &text[1..text_start], &text[text_start..].trim()).into()
}
