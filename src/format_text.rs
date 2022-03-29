use anyhow::Result;
use dockerfile_parser::Dockerfile;
use dprint_core::configuration::resolve_new_line_kind;
use dprint_core::formatting::PrintOptions;
use dprint_core::plugins::FormatResult;
use std::path::Path;

use crate::configuration::Configuration;
use crate::generation::generate;

pub fn format_text(_file_path: &Path, text: &str, config: &Configuration) -> FormatResult {
  let node = parse_node(text)?;

  let result = dprint_core::formatting::format(|| generate(&node, text, config), config_to_print_options(text, config));
  if result == text {
    Ok(None)
  } else {
    Ok(Some(result))
  }
}

#[cfg(feature = "tracing")]
pub fn trace_file(_file_path: &Path, text: &str, config: &Configuration) -> dprint_core::formatting::TracingResult {
  let node = parse_node(text).unwrap();

  dprint_core::formatting::trace_printing(|| generate(&node, text, config), config_to_print_options(text, config))
}

fn parse_node(text: &str) -> Result<Dockerfile> {
  Ok(Dockerfile::parse(text)?)
}

fn config_to_print_options(text: &str, config: &Configuration) -> PrintOptions {
  PrintOptions {
    indent_width: 1,
    max_width: config.line_width,
    use_tabs: false,
    new_line_text: resolve_new_line_kind(text, config.new_line_kind),
  }
}
