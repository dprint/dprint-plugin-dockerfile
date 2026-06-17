use dprint_core::formatting::ir_helpers::SingleLineOptions;
use dprint_core::formatting::ir_helpers::gen_from_raw_string;
use dprint_core::formatting::*;
use dprint_core_macros::sc;

use super::context::Context;
use super::helpers::*;
use crate::ast::*;
use crate::configuration::Configuration;

pub fn generate(file: &Dockerfile, text: &str, config: &Configuration) -> PrintItems {
  let mut context = Context::new(text, file, config);
  let mut items = PrintItems::new();
  let top_level_nodes = context.gen_nodes_with_comments(0, text.len(), true, file.instructions.iter().map(|i| i.into()));

  for (i, node) in top_level_nodes.iter().enumerate() {
    let node_items = gen_node(node.clone(), &mut context);
    // safety net: never drop a comment. some instructions discard comments that
    // follow a line continuation (the parser's arg_ws consumes them); recover
    // any that weren't emitted and place them just before the instruction.
    items.extend(recover_dropped_comments(node, &mut context));
    items.extend(node_items);
    items.push_signal(Signal::NewLine);
    if let Some(next_node) = top_level_nodes.get(i + 1) {
      let text_between = &text[node.span().end..next_node.span().start];
      if text_between.chars().filter(|c| *c == '\n').count() > 1 {
        items.push_signal(Signal::NewLine);
      }
    }
  }

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
    Node::FromFlag(node) => gen_from_flag(node, context),
    Node::Label(node) => gen_label_instruction(node, context),
    Node::LabelLabel(node) => gen_label(node, context),
    Node::Misc(node) => gen_misc_instruction(node, context),
    Node::Shell(node) => gen_shell_instruction(node, context),
    Node::Onbuild(node) => gen_onbuild_instruction(node, context),
    Node::Healthcheck(node) => gen_healthcheck_instruction(node, context),
    Node::Heredoc(node) => gen_heredoc_instruction(node, context),
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

  items.push_sc(sc!("ARG "));
  items.extend(gen_node((&node.name).into(), context));

  if let Some(value) = &node.value {
    items.push_sc(sc!("="));
    items.extend(gen_node(value.into(), context));
  }

  items
}

fn gen_cmd_instruction<'a>(node: &'a CmdInstruction, context: &mut Context<'a>) -> PrintItems {
  let mut items = PrintItems::new();
  items.push_sc(sc!("CMD "));
  items.extend(match &node.expr {
    ShellOrExecExpr::Exec(node) => gen_node(node.into(), context),
    ShellOrExecExpr::Shell(node) => gen_node(node.into(), context),
  });
  items
}

fn gen_copy_instruction<'a>(node: &'a CopyInstruction, context: &mut Context<'a>) -> PrintItems {
  let mut items = PrintItems::new();
  let prefix = sc!("COPY ");
  items.push_sc(prefix);

  match &node.args {
    CopyArgs::Exec(array) => {
      for flag in &node.flags {
        items.extend(gen_node(flag.into(), context));
        items.push_sc(sc!(" "));
      }
      items.extend(gen_node(array.into(), context));
    }
    CopyArgs::Paths { sources, destination } => {
      let value_nodes = node
        .flags
        .iter()
        .map(|flag| flag.into())
        .chain(sources.iter().map(|source| source.into()))
        .chain(std::iter::once(destination.into()));
      let nodes = context.gen_nodes_with_comments(node.span.start, node.span.end, false, value_nodes);

      if nodes.iter().any(|node| node.is_comment()) {
        // preserve comments by breaking onto multiple lines, aligned with the arguments
        items.extend(gen_multi_line_items(nodes, prefix.text.chars().count() as u32, context));
      } else {
        // keep everything on a single line
        for (i, node) in nodes.into_iter().enumerate() {
          if i > 0 {
            items.push_sc(sc!(" "));
          }
          items.extend(gen_node(node, context));
        }
      }
    }
  }
  items
}

fn gen_entrypoint_instruction<'a>(node: &'a EntrypointInstruction, context: &mut Context<'a>) -> PrintItems {
  let mut items = PrintItems::new();
  items.push_sc(sc!("ENTRYPOINT "));
  items.extend(match &node.expr {
    ShellOrExecExpr::Exec(node) => gen_node(node.into(), context),
    ShellOrExecExpr::Shell(node) => gen_node(node.into(), context),
  });
  items
}

fn gen_env_instruction<'a>(node: &'a EnvInstruction, context: &mut Context<'a>) -> PrintItems {
  let mut items = PrintItems::new();
  let nodes = context.gen_nodes_with_comments(node.span.start, node.span.end, false, node.vars.iter().map(|i| i.into()));
  let prefix = sc!("ENV ");
  items.push_sc(prefix);
  items.extend(gen_multi_line_items(nodes, prefix.text.chars().count() as u32, context));
  items
}

fn gen_env_var<'a>(node: &'a EnvVar, context: &mut Context<'a>) -> PrintItems {
  let mut items = PrintItems::new();
  items.extend(gen_node((&node.key).into(), context));
  items.push_sc(sc!("="));
  items.extend(gen_node((&node.value).into(), context));
  items
}

fn gen_from_instruction<'a>(node: &'a FromInstruction, context: &mut Context<'a>) -> PrintItems {
  let mut items = PrintItems::new();
  items.push_sc(sc!("FROM "));
  for flag in &node.flags {
    items.extend(gen_node(flag.into(), context));
    items.push_sc(sc!(" "));
  }
  items.extend(gen_node((&node.image).into(), context));
  if let Some(alias) = &node.alias {
    items.push_sc(sc!(" AS "));
    items.extend(gen_node(alias.into(), context));
  }
  items
}

fn gen_from_flag<'a>(node: &'a FromFlag, context: &mut Context<'a>) -> PrintItems {
  let mut items = PrintItems::new();
  items.push_sc(sc!("--"));
  items.extend(gen_node((&node.name).into(), context));
  items.push_sc(sc!("="));
  items.extend(gen_node((&node.value).into(), context));
  items
}

fn gen_label_instruction<'a>(node: &'a LabelInstruction, context: &mut Context<'a>) -> PrintItems {
  let mut items = PrintItems::new();
  let prefix = sc!("LABEL ");
  items.push_sc(prefix);
  // route through gen_nodes_with_comments so comments between labels are kept
  let nodes = context.gen_nodes_with_comments(node.span.start, node.span.end, false, node.labels.iter().map(|l| l.into()));
  items.extend(gen_multi_line_items(nodes, prefix.text.chars().count() as u32, context));
  items
}

fn gen_label<'a>(node: &'a Label, context: &mut Context<'a>) -> PrintItems {
  let mut items = PrintItems::new();
  items.extend(gen_node((&node.name).into(), context));
  items.push_sc(sc!("="));
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
  let force_use_new_lines = nodes_with_line_index.len() > 1
    && (nodes_with_line_index[0].1 < nodes_with_line_index[1].1 || nodes_with_line_index.iter().any(|(node, _)| node.is_comment()));
  let space_continuation = space_continuation(context.escape());

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
              items.push_sc(space_continuation);
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
      single_line_options: SingleLineOptions {
        space_at_start: false,
        space_at_end: false,
        separator: Signal::SpaceOrNewLine.into(),
      },
      indent_width: 0_u8,
      multi_line_options: ir_helpers::MultiLineOptions::same_line_no_indent(),
      force_possible_newline_at_start: false,
    },
  )
  .items
}

fn gen_misc_instruction<'a>(node: &'a MiscInstruction, context: &mut Context<'a>) -> PrintItems {
  let mut items = PrintItems::new();
  items.extend(gen_node((&node.instruction).into(), context));
  items.push_sc(sc!(" "));
  items.extend(gen_node((&node.arguments).into(), context));
  items
}

fn gen_run_instruction<'a>(node: &'a RunInstruction, context: &mut Context<'a>) -> PrintItems {
  let mut items = PrintItems::new();

  items.push_sc(sc!("RUN "));
  items.extend(match &node.expr {
    ShellOrExecExpr::Exec(node) => gen_node(node.into(), context),
    ShellOrExecExpr::Shell(node) => gen_node(node.into(), context),
  });

  items
}

fn gen_shell_instruction<'a>(node: &'a ShellInstruction, context: &mut Context<'a>) -> PrintItems {
  let mut items = PrintItems::new();
  items.push_sc(sc!("SHELL "));
  items.extend(match &node.expr {
    ShellOrExecExpr::Exec(node) => gen_node(node.into(), context),
    ShellOrExecExpr::Shell(node) => gen_node(node.into(), context),
  });
  items
}

fn gen_onbuild_instruction<'a>(node: &'a OnbuildInstruction, context: &mut Context<'a>) -> PrintItems {
  let mut items = PrintItems::new();
  items.push_sc(sc!("ONBUILD "));
  items.extend(gen_node((&*node.instruction).into(), context));
  items
}

fn gen_healthcheck_instruction<'a>(node: &'a HealthcheckInstruction, context: &mut Context<'a>) -> PrintItems {
  let mut items = PrintItems::new();
  items.push_sc(sc!("HEALTHCHECK"));
  for flag in &node.flags {
    items.push_sc(sc!(" --"));
    items.extend(gen_node((&flag.name).into(), context));
    items.push_sc(sc!("="));
    items.extend(gen_node((&flag.value).into(), context));
  }
  match &node.cmd {
    Some(instruction) => {
      // the byte position just after the last token preceding the nested command
      let prev_end = node.flags.last().map(|f| f.span.end).unwrap_or(node.span.start + "HEALTHCHECK".len());
      let gap = context.span_text(&Span::new(prev_end, instruction.span().start));
      // keep an author-written line continuation before the nested command (e.g.
      // `HEALTHCHECK --opt \` then `CMD ...`) rather than collapsing it (#29)
      match continuation_indent(gap, context.escape()) {
        Some(indent) => {
          items.push_sc(space_continuation(context.escape()));
          items.push_signal(Signal::NewLine);
          if !indent.is_empty() {
            items.extend(gen_from_raw_string(indent));
          }
        }
        None => items.push_sc(sc!(" ")),
      }
      items.extend(gen_node((&**instruction).into(), context));
    }
    // a continuation before NONE (`HEALTHCHECK --opt \` then `NONE`) is collapsed
    // since NONE is short and never benefits from its own line
    None => items.push_sc(sc!(" NONE")),
  }
  items
}

/// If the text between an instruction's last token and its nested command holds
/// a line continuation, returns the indentation of the continued line (the
/// whitespace following the final newline) so the break can be preserved (#29).
/// Returns `None` when there is no continuation, signalling a single space.
fn continuation_indent(gap: &str, escape: char) -> Option<&str> {
  let first_newline = gap.find(['\n', '\r'])?;
  // the segment before the newline must end in the escape character
  if !gap[..first_newline].trim_end().ends_with(escape) {
    return None;
  }
  // the nested command sits after the final newline; reuse its leading whitespace
  let after = &gap[gap.rfind(['\n', '\r']).unwrap() + 1..];
  let indent_end = after.find(|c: char| c != ' ' && c != '\t').unwrap_or(after.len());
  Some(&after[..indent_end])
}

fn gen_heredoc_instruction<'a>(node: &'a HeredocInstruction, context: &mut Context<'a>) -> PrintItems {
  let mut items = PrintItems::new();
  // the first line is a normal instruction and is formatted as such
  items.extend(gen_node((&*node.instruction).into(), context));
  // the heredoc body and its closing delimiter(s) are preserved verbatim
  items.push_signal(Signal::NewLine);
  items.extend(gen_from_raw_string(&node.body));
  items
}

fn gen_string_array<'a>(node: &'a StringArray, context: &mut Context<'a>) -> PrintItems {
  let mut items = PrintItems::new();
  items.push_sc(sc!("["));
  for (i, element) in node.elements.iter().enumerate() {
    items.extend(gen_node(element.into(), context));
    if i < node.elements.len() - 1 {
      items.push_sc(sc!(", "));
    }
  }
  items.push_sc(sc!("]"));
  items
}

/// The line-continuation marker for the file's escape character.
fn continuation(escape: char) -> &'static StringContainer {
  if escape == '`' { sc!("`") } else { sc!("\\") }
}

/// The line-continuation marker preceded by a separating space.
fn space_continuation(escape: char) -> &'static StringContainer {
  if escape == '`' { sc!(" `") } else { sc!(" \\") }
}

fn gen_breakable_string<'a>(node: &'a BreakableString, context: &mut Context<'a>) -> PrintItems {
  let mut items = PrintItems::new();
  let is_parent_env_var = matches!(context.parent(), Some(Node::EnvVar(_)));
  let span_text = context.span_text(&node.span);
  let use_quotes = is_parent_env_var && span_text.contains(' ');
  let previous_gen_string_content = context.gen_string_content;
  context.gen_string_content = use_quotes;

  // collapse runs of insignificant whitespace in shell commands (#8), but not
  // in the quoted env-var form (which preserves its content verbatim)
  let is_shell_command = matches!(context.parent(), Some(Node::Run(_) | Node::Cmd(_) | Node::Entrypoint(_)));
  let previous_collapse = context.collapse_shell_ws;
  let previous_quote = context.shell_quote;
  context.collapse_shell_ws = is_shell_command && !use_quotes;
  context.shell_quote = None;
  let continuation = continuation(context.escape());
  let space_continuation = space_continuation(context.escape());

  if use_quotes {
    items.push_sc(sc!("\""));
  }
  // when the breakable starts with a comment (e.g. `RUN \` followed by a
  // comment line), emit the line continuation so the comment stays attached to
  // the instruction instead of being dropped or turning the rest into a comment
  if matches!(node.components.first(), Some(BreakableStringComponent::Comment(_))) {
    items.push_sc(continuation);
    items.push_signal(Signal::NewLine);
  }
  for (i, component) in node.components.iter().enumerate() {
    // comments lose their leading whitespace when parsed, so align them
    // with the surrounding arguments by reusing their indentation
    if matches!(component, BreakableStringComponent::Comment(_)) {
      let indentation = comment_indentation(&node.components, i);
      if !indentation.is_empty() {
        // via gen_from_raw_string so a tab indent becomes a Tab signal
        items.extend(gen_from_raw_string(indentation));
      }
    }
    items.extend(gen_node(component.into(), context));
    if i < node.components.len() - 1 {
      if let BreakableStringComponent::String(text) = component {
        // when the component ends inside a quote, any trailing whitespace is
        // part of the (kept) string content, so don't add a separator space
        let ends_in_quote = context.collapse_shell_ws && context.shell_quote.is_some();
        if !use_quotes && !ends_in_quote && text.content.ends_with(" ") {
          items.push_sc(space_continuation);
        } else {
          items.push_sc(continuation);
        }
      }
      items.push_signal(Signal::NewLine);
    }
  }
  if use_quotes {
    items.push_sc(sc!("\""));
  }

  context.gen_string_content = previous_gen_string_content;
  context.collapse_shell_ws = previous_collapse;
  context.shell_quote = previous_quote;
  items
}

/// Determines the indentation to use for a comment within a breakable string by
/// reusing the leading whitespace of the closest surrounding string component.
fn comment_indentation(components: &[BreakableStringComponent], index: usize) -> &str {
  let following = components[index + 1..].iter().find_map(string_leading_whitespace);
  if let Some(whitespace) = following {
    return whitespace;
  }
  components[..index].iter().rev().find_map(string_leading_whitespace).unwrap_or("")
}

fn string_leading_whitespace(component: &BreakableStringComponent) -> Option<&str> {
  match component {
    BreakableStringComponent::String(s) => Some(&s.content[..s.content.len() - s.content.trim_start().len()]),
    _ => None,
  }
}

fn gen_string<'a>(node: &'a SpannedString, context: &mut Context<'a>) -> PrintItems {
  let mut items = PrintItems::new();
  if context.gen_string_content {
    // don't trim this because it's the content
    items.extend(gen_from_raw_string(&node.content));
    return items;
  }

  // only the leading content string (right after the instruction prefix) has
  // its indentation trimmed; later lines keep their leading whitespace. when
  // the breakable starts with a comment there is no leading content string, so
  // every string preserves its indentation
  let should_trim = if let Some(Node::BreakableString(parent)) = context.parent() {
    matches!(parent.components.first(), Some(BreakableStringComponent::String(str)) if str.span == node.span)
  } else {
    true
  };
  let raw = context.span_text(&node.span);

  if context.collapse_shell_ws {
    // run the quote-aware collapse on the raw text so significant whitespace
    // inside a quote is never stripped by a blind trim
    let collapsed = collapse_shell_whitespace(raw, should_trim, &mut context.shell_quote);
    // trailing whitespace outside a quote is just a separator (represented by
    // the line-continuation marker), so drop it; inside a quote it is part of
    // the string and must be kept
    let text = if context.shell_quote.is_none() {
      collapsed.trim_end()
    } else {
      collapsed.as_str()
    };
    items.extend(gen_from_raw_string(text));
  } else {
    let text = if should_trim { raw.trim() } else { raw.trim_end() };
    items.extend(gen_from_raw_string(text));
  }
  items
}

/// Collapses runs of two or more insignificant whitespace characters into a
/// single space within shell command text, leaving whitespace inside quotes and
/// after a backslash escape untouched. `quote` tracks the open quote across the
/// breakable string's components. Leading whitespace is kept as indentation
/// unless `drop_leading` is set (the leading content line, whose indentation is
/// trimmed away).
fn collapse_shell_whitespace(text: &str, drop_leading: bool, quote: &mut Option<char>) -> String {
  let mut out = String::with_capacity(text.len());
  let mut chars = text.chars().peekable();

  // leading whitespace outside a quote is indentation: keep it, or drop it for
  // the leading content line
  if quote.is_none() {
    while matches!(chars.peek(), Some(' ' | '\t')) {
      let c = chars.next().unwrap();
      if !drop_leading {
        out.push(c);
      }
    }
  }

  while let Some(c) = chars.next() {
    match *quote {
      Some('\'') => {
        out.push(c);
        if c == '\'' {
          *quote = None;
        }
      }
      Some(_) => {
        // inside a double quote: a backslash escapes the next character
        out.push(c);
        if c == '\\' {
          if let Some(next) = chars.next() {
            out.push(next);
          }
        } else if c == '"' {
          *quote = None;
        }
      }
      None => match c {
        '\'' | '"' => {
          out.push(c);
          *quote = Some(c);
        }
        // an escaped character (including `\ `) is kept verbatim
        '\\' => {
          out.push(c);
          if let Some(next) = chars.next() {
            out.push(next);
          }
        }
        ' ' | '\t' => {
          let mut count = 1;
          while matches!(chars.peek(), Some(' ' | '\t')) {
            chars.next();
            count += 1;
          }
          if count > 1 {
            out.push(' ');
          } else {
            out.push(c);
          }
        }
        _ => out.push(c),
      },
    }
  }
  out
}

fn gen_copy_flag<'a>(node: &'a CopyFlag, context: &mut Context<'a>) -> PrintItems {
  // ex: --from=foo
  let mut items = PrintItems::new();
  items.push_sc(sc!("--"));
  items.extend(gen_node((&node.name).into(), context));
  items.push_sc(sc!("="));
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

/// Recovers any comment inside an instruction's span that its generator didn't
/// already emit, so comments following a line continuation are never dropped.
/// Heredoc bodies are excluded — they are verbatim and may contain `#` lines.
fn recover_dropped_comments<'a>(node: &Node<'a>, context: &mut Context<'a>) -> PrintItems {
  let mut items = PrintItems::new();
  if node.is_comment() {
    return items;
  }
  let span = match node {
    Node::Heredoc(heredoc) => heredoc.instruction.span(),
    _ => node.span(),
  };
  let comments = {
    let interior = &context.text[span.start..span.end];
    parse_comments(interior, span.start)
  };
  for comment in comments {
    if !context.handled_comments.contains(&comment.span.start) {
      items.extend(gen_comment(&comment, context));
    }
  }
  items
}

fn gen_comment_text(text: &str) -> PrintItems {
  // comments always retain their leading `#`(s); split them from the body text
  let text_start = text.find(|c| c != '#').unwrap_or(text.len());
  let comment_chars = &text[1..text_start];
  let end_text = &text[text_start..].trim();
  let comment = if end_text.is_empty() {
    format!("#{}", comment_chars)
  } else {
    format!("#{} {}", comment_chars, end_text)
  };
  // via gen_from_raw_string so an interior tab becomes a Tab signal rather than
  // a raw tab (which the printer rejects)
  gen_from_raw_string(&comment)
}
