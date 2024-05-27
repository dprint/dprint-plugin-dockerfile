use anyhow::Result;
use dockerfile_parser::Dockerfile;
use dprint_core::configuration::resolve_new_line_kind;
use dprint_core::formatting::PrintOptions;
use std::path::Path;

use crate::configuration::Configuration;
use crate::generation::generate;

pub fn format_text(_file_path: &Path, text: &str, config: &Configuration) -> Result<Option<String>> {
  let result = format_inner(text, config)?;
  if result == text {
    Ok(None)
  } else {
    Ok(Some(result))
  }
}

fn format_inner(text: &str, config: &Configuration) -> Result<String> {
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

fn parse_node(text: &str) -> Result<Dockerfile> {
  Ok(Dockerfile::parse(text)?)
}

fn strip_bom(text: &str) -> &str {
  text.strip_prefix("\u{FEFF}").unwrap_or(text)
}

fn config_to_print_options(text: &str, config: &Configuration) -> PrintOptions {
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
}
