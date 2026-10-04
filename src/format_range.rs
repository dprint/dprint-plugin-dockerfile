use std::ops::Range;
use std::path::Path;

use dprint_core::configuration::NewLineKind;
use dprint_core::configuration::resolve_new_line_kind;
use dprint_core::formatting::PrintOptions;

use crate::ast::Dockerfile;
use crate::configuration::Configuration;
use crate::error::FormatError;
use crate::format_text::config_to_print_options;
use crate::format_text::format_text;
use crate::format_text::parse_node;
use crate::format_text::strip_bom;
use crate::generation::Node;
use crate::generation::generate_nodes;
use crate::generation::top_level_nodes;

/// Formats only the part of the text within the provided byte range.
///
/// The range is widened to the lines of the instructions and comments it
/// touches, and the text outside of those is left as it was, including its
/// line endings. A range that touches everything from the first instruction
/// or comment to the last formats the whole file, as does any range in a file
/// with a lone carriage return, and one that only touches blank lines formats
/// nothing.
pub fn format_text_range(file_path: &Path, text: &str, range: Range<usize>, config: &Configuration) -> Result<Option<String>, FormatError> {
  let body = strip_bom(text);
  let bom_len = text.len() - body.len();
  let end = range.end.saturating_sub(bom_len).min(body.len());
  let range = range.start.saturating_sub(bom_len).min(end)..end;
  // a range covering the file formats it, even when there's nothing but blank
  // lines in it. a lone carriage return breaks a line for the parser, but not
  // for the line lookups here, which is rare enough to leave to formatting the
  // whole file
  if range == (0..body.len()) || has_lone_carriage_return(body) {
    return format_text(file_path, text, config);
  }

  let file = parse_node(body)?;
  let nodes = top_level_nodes(&file, body);
  let indexes = match find_target(&nodes, &file, &range) {
    Target::File => return format_text(file_path, text, config),
    Target::Nothing => return Ok(None),
    Target::Nodes(indexes) => indexes,
  };
  let replaced = line_start(&file, nodes[indexes.start].span().start)..line_end(&file, nodes[indexes.end - 1].span().end);
  let formatted = dprint_core::formatting::format(
    || generate_nodes(&nodes, indexes, &file, body, config),
    PrintOptions {
      // keep the file's line endings since changing them is up to formatting
      // the whole file
      new_line_text: resolve_new_line_kind(body, NewLineKind::Auto),
      ..config_to_print_options(body, config)
    },
  );

  let result = format!("{}{}{}", &text[..bom_len + replaced.start], formatted, &text[bom_len + replaced.end..]);
  if result == text { Ok(None) } else { Ok(Some(result)) }
}

enum Target {
  /// The range touches both the first and last node of the file.
  File,
  /// The range is outside of the nodes or only touches the blank lines
  /// between them.
  Nothing,
  /// Indexes of the top-level nodes to format.
  Nodes(Range<usize>),
}

fn find_target(nodes: &[Node], file: &Dockerfile, range: &Range<usize>) -> Target {
  let mut touched = nodes
    .iter()
    .enumerate()
    .filter(|(_, node)| {
      let span = node.span();
      touches(&(line_start(file, span.start)..line_end(file, span.end)), range)
    })
    .map(|(index, _)| index);
  let Some(first) = touched.next() else {
    return Target::Nothing;
  };
  let mut last = touched.next_back().unwrap_or(first);
  // whole lines are replaced, so bring in the comment following an instruction
  // on its last line, which isn't touched by a range on the lines before it
  while last + 1 < nodes.len() && file.line_index(nodes[last + 1].span().start) == file.line_index(nodes[last].span().end) {
    last += 1;
  }
  // the start and end of the file are only formatted along with all of it
  if first == 0 && last == nodes.len() - 1 {
    Target::File
  } else {
    Target::Nodes(first..last + 1)
  }
}

fn touches(lines: &Range<usize>, range: &Range<usize>) -> bool {
  if range.is_empty() {
    lines.start <= range.start && range.start <= lines.end
  } else {
    range.start < lines.end && range.end > lines.start
  }
}

fn line_start(file: &Dockerfile, pos: usize) -> usize {
  file.line_start(file.line_index(pos))
}

/// The end of the line the position is on, without the line break.
fn line_end(file: &Dockerfile, pos: usize) -> usize {
  let text = file.content;
  let end = text[pos..].find('\n').map(|index| pos + index).unwrap_or(text.len());
  // checked from before the position since the span of a comment includes
  // the carriage return of its line break
  if text[..end].ends_with('\r') { end - 1 } else { end }
}

fn has_lone_carriage_return(text: &str) -> bool {
  text.match_indices('\r').any(|(index, _)| text.as_bytes().get(index + 1) != Some(&b'\n'))
}

#[cfg(test)]
mod test {
  use super::*;
  use crate::configuration::ConfigurationBuilder;

  #[test]
  fn keeps_bom_outside_range() {
    // the spec files can't express this since editors strip the bom
    let config = ConfigurationBuilder::new().build();
    let output = format_b(&config, "\u{FEFF}FROM   a\nRUN   b\nRUN   c\n");
    assert_eq!(output.as_deref(), Some("\u{FEFF}FROM   a\nRUN b\nRUN   c\n"));
  }

  #[test]
  fn keeps_file_line_endings() {
    // the spec files can't express this since they normalize line endings
    let config = ConfigurationBuilder::new().build();
    let output = format_b(&config, "FROM   a\r\nRUN   b \\\r\n  x   \r\nRUN   c\r\n");
    assert_eq!(output.as_deref(), Some("FROM   a\r\nRUN b \\\r\n  x\r\nRUN   c\r\n"));

    let config = ConfigurationBuilder::new().new_line_kind(NewLineKind::CarriageReturnLineFeed).build();
    let output = format_b(&config, "FROM   a\nRUN   b \\\n  x   \nRUN   c\n");
    assert_eq!(output.as_deref(), Some("FROM   a\nRUN b \\\n  x\nRUN   c\n"));
  }

  #[test]
  fn formats_whole_file_with_lone_carriage_returns() {
    let config = ConfigurationBuilder::new().build();
    let text = "FROM   a\rRUN   b\nRUN   c\n";
    assert_eq!(format_b(&config, text), format_text(Path::new("Dockerfile"), text, &config).unwrap());
  }

  #[test]
  fn clamps_range_past_end_of_file() {
    let config = ConfigurationBuilder::new().build();
    let text = "FROM   a\nRUN   b\n";
    let output = format_text_range(Path::new("Dockerfile"), text, text.find("RUN").unwrap()..100, &config).unwrap();
    assert_eq!(output.as_deref(), Some("FROM   a\nRUN b\n"));
  }

  #[test]
  fn keeps_carriage_return_after_comment() {
    // the span of a comment includes the carriage return
    let config = ConfigurationBuilder::new().build();
    let output = format_b(&config, "FROM   a\r\n#b\r\nRUN   c\r\n");
    assert_eq!(output.as_deref(), Some("FROM   a\r\n# b\r\nRUN   c\r\n"));
    let output = format_b(&config, "FROM   a\r\nARG   b=1   #c\r\nRUN   d\r\n");
    assert_eq!(output.as_deref(), Some("FROM   a\r\nARG b=1\r\n# c\r\nRUN   d\r\n"));
    assert_eq!(format_b(&config, "FROM   a\r\n# b\r\nRUN   c\r\n"), None);
  }

  #[test]
  fn keeps_file_line_endings_between_nodes_and_in_heredocs() {
    let config = ConfigurationBuilder::new().build();
    let text = "FROM   a\r\nRUN   b\r\n\r\n\r\nRUN   c\r\nRUN   d\r\n";
    let range = text.find('b').unwrap()..text.find('c').unwrap();
    let output = format_text_range(Path::new("Dockerfile"), text, range, &config).unwrap();
    assert_eq!(output.as_deref(), Some("FROM   a\r\nRUN b\r\n\r\nRUN c\r\nRUN   d\r\n"));

    let output = format_b(&config, "FROM   a\r\nRUN   <<EOF\r\necho   b\r\nEOF\r\nRUN   c\r\n");
    assert_eq!(output.as_deref(), Some("FROM   a\r\nRUN <<EOF\r\necho   b\r\nEOF\r\nRUN   c\r\n"));
  }

  #[test]
  fn formats_last_node_of_file_without_final_newline() {
    // the spec files can't express this well since they end the text with a newline
    let config = ConfigurationBuilder::new().build();
    assert_eq!(format_b(&config, "FROM   a\nRUN   b").as_deref(), Some("FROM   a\nRUN b"));
    assert_eq!(format_b(&config, "FROM   a\r\nRUN   b").as_deref(), Some("FROM   a\r\nRUN b"));
    assert_eq!(format_b(&config, "FROM   a\n#b").as_deref(), Some("FROM   a\n# b"));
    assert_eq!(
      format_b(&config, "FROM   a\nRUN   <<EOF\necho   b\nEOF").as_deref(),
      Some("FROM   a\nRUN <<EOF\necho   b\nEOF")
    );
    // cursor at the end of the file
    let text = "FROM   a\nRUN   c";
    assert_eq!(format(&config, text, text.len()..text.len()).as_deref(), Some("FROM   a\nRUN c"));
  }

  #[test]
  fn offsets_range_by_bom() {
    let config = ConfigurationBuilder::new().build();
    let text = "\u{FEFF}FROM   a\nRUN   b\nRUN   c\n";
    let b_pos = text.find('b').unwrap();
    // from the end of one line to the start of the next
    assert_eq!(format(&config, text, b_pos..b_pos + 3).as_deref(), Some("\u{FEFF}FROM   a\nRUN b\nRUN c\n"));
    // only the line break after it
    assert_eq!(format(&config, text, b_pos + 1..b_pos + 2), None);
    // within the bom
    assert_eq!(format(&config, text, 0..3).as_deref(), Some("\u{FEFF}FROM a\nRUN   b\nRUN   c\n"));
    assert_eq!(format(&config, text, 1..1).as_deref(), Some("\u{FEFF}FROM a\nRUN   b\nRUN   c\n"));
    // everything, with and without the bom
    let formatted_file = format_text(Path::new("Dockerfile"), text, &config).unwrap();
    assert_eq!(format(&config, text, 0..text.len()), formatted_file);
    assert_eq!(format(&config, text, 3..text.len()), formatted_file);
    assert_eq!(format(&config, text, 4..text.len() - 2), formatted_file);
    // between the nodes
    assert_eq!(format(&config, "\u{FEFF}FROM   a\n\n\nRUN   c\n", 13..13), None);
  }

  #[test]
  #[allow(clippy::reversed_empty_ranges)]
  fn clamps_range_start() {
    let config = ConfigurationBuilder::new().build();
    let text = "FROM   a\nRUN   b\nRUN   c\n";
    // a start past the end is a cursor at the end
    assert_eq!(format(&config, text, 12..3).as_deref(), Some("FROM a\nRUN   b\nRUN   c\n"));
    // past the final newline there's nothing to format
    assert_eq!(format(&config, text, 100..200), None);
    assert_eq!(format(&config, "FROM   a\nRUN   b", 100..200).as_deref(), Some("FROM   a\nRUN b"));
  }

  #[test]
  fn formats_file_without_nodes_only_when_range_covers_it() {
    let config = ConfigurationBuilder::new().build();
    assert_eq!(format(&config, "\n\n\n", 0..3).as_deref(), Some(""));
    assert_eq!(format(&config, "\n\n\n", 0..100).as_deref(), Some(""));
    assert_eq!(format(&config, "\n\n\n", 0..2), None);
    assert_eq!(format(&config, "\n\n\n", 1..1), None);
    assert_eq!(format(&config, "", 0..0), None);
  }

  #[test]
  fn returns_none_when_nothing_changes() {
    // the spec files can't tell this apart from returning the same text
    let config = ConfigurationBuilder::new().build();
    assert_eq!(format_b(&config, "FROM   a\nRUN b\nRUN   c\n"), None);
    assert_eq!(format(&config, "FROM   a\n\n\nRUN   c\n", 10..10), None);
  }

  #[test]
  fn treats_whitespace_only_lines_as_blank() {
    // the spec files can't express this well since editors strip trailing whitespace
    let config = ConfigurationBuilder::new().build();
    let text = "FROM   a\nRUN   b\n  \t \nRUN   c\nRUN   d\n";
    let output = format(&config, text, text.find('b').unwrap()..text.find('c').unwrap());
    assert_eq!(output.as_deref(), Some("FROM   a\nRUN b\n\nRUN c\nRUN   d\n"));
    let pos = text.find('\t').unwrap();
    assert_eq!(format(&config, text, pos..pos), None);
  }

  fn format(config: &Configuration, text: &str, range: Range<usize>) -> Option<String> {
    format_text_range(Path::new("Dockerfile"), text, range, config).unwrap()
  }

  fn format_b(config: &Configuration, text: &str) -> Option<String> {
    let start = text.find('b').unwrap();
    format_text_range(Path::new("Dockerfile"), text, start..start + 1, config).unwrap()
  }
}
