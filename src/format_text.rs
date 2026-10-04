use dprint_core::configuration::resolve_new_line_kind;
use dprint_core::formatting::PrintOptions;
use std::path::Path;

use crate::ast::Dockerfile;
use crate::configuration::Configuration;
use crate::error::FormatError;
use crate::generation::generate;

pub fn format_text(_file_path: &Path, text: &str, config: &Configuration) -> Result<Option<String>, FormatError> {
  let result = format_inner(text, config)?;
  if result == text { Ok(None) } else { Ok(Some(result)) }
}

fn format_inner(text: &str, config: &Configuration) -> Result<String, FormatError> {
  let text = strip_bom(text);
  let node = parse_node(text)?;

  Ok(dprint_core::formatting::format(
    || generate(&node, text, config),
    config_to_print_options(text, config),
  ))
}

#[cfg(feature = "tracing")]
pub fn trace_file(_file_path: &Path, text: &str, config: &Configuration) -> dprint_core::formatting::TracingResult {
  let node = parse_node(text).unwrap();

  dprint_core::formatting::trace_printing(|| generate(&node, text, config), config_to_print_options(text, config))
}

pub(crate) fn parse_node(text: &str) -> Result<Dockerfile<'_>, FormatError> {
  Ok(Dockerfile::parse(text)?)
}

pub(crate) fn strip_bom(text: &str) -> &str {
  text.strip_prefix("\u{FEFF}").unwrap_or(text)
}

pub(crate) fn config_to_print_options(text: &str, config: &Configuration) -> PrintOptions {
  PrintOptions {
    indent_width: 1,
    max_width: config.line_width,
    use_tabs: false,
    new_line_text: resolve_new_line_kind(text, config.new_line_kind),
  }
}

#[cfg(test)]
mod test {
  use super::*;

  #[test]
  fn strips_bom() {
    for input_text in ["\u{FEFF}FROM example:12.16.1\n", "\u{FEFF}FROM    example:12.16.1\n"] {
      let text = format_text(
        &std::path::PathBuf::from("test.dockerfile"),
        input_text,
        &crate::configuration::ConfigurationBuilder::new().build(),
      )
      .unwrap()
      .unwrap();
      assert_eq!(text, "FROM example:12.16.1\n");
    }
  }

  #[test]
  fn indents_stages_with_carriage_returns() {
    // a `\r` is never part of the indentation and never reaches the dedent
    let text = format_with(
      "FROM alpine\r\n    RUN foo \\\r\n      bar\r\n",
      crate::configuration::ConfigurationBuilder::new().indent_stages(true),
    );
    assert_eq!(text, "FROM alpine\n  RUN foo \\\n    bar\n");
  }

  #[test]
  fn indent_width_of_zero_does_not_indent_stages() {
    let text = format_with(
      "FROM alpine\nRUN echo hi\n",
      crate::configuration::ConfigurationBuilder::new().indent_stages(true).indent_width(0),
    );
    assert_eq!(text, "FROM alpine\nRUN echo hi\n");
  }

  #[test]
  fn huge_indent_width_is_capped_instead_of_overflowing_the_printer() {
    // the printer stores its indent level in a `u8`, so the width is capped
    let text = format_with(
      "FROM alpine\nENV A=1 \\\n  B=2\n",
      crate::configuration::ConfigurationBuilder::new().indent_stages(true).indent_width(255),
    );
    let indent = " ".repeat(200);
    assert_eq!(text, format!("FROM alpine\n{0}ENV A=1 \\\n{0}    B=2\n", indent));
  }

  /// Formats the text, resolving the "already formatted" result to the input.
  fn format_with(text: &str, builder: &mut crate::configuration::ConfigurationBuilder) -> String {
    format_text(&std::path::PathBuf::from("Dockerfile"), text, &builder.build())
      .unwrap()
      .unwrap_or_else(|| text.to_string())
  }

  #[test]
  fn comment_with_interior_tab_does_not_panic() {
    // a tab inside a comment must be emitted as a tab signal, not a raw tab
    // that the printer rejects
    let result = format_text(
      &std::path::PathBuf::from("Dockerfile"),
      "# a\tb\nFROM x\n",
      &crate::configuration::ConfigurationBuilder::new().build(),
    );
    assert!(result.is_ok());
  }
}
