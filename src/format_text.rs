use dprint_core::types::ErrBox;
use std::path::Path;

use crate::configuration::Configuration;

pub fn format_text(_file_path: &Path, text: &str, _config: &Configuration) -> Result<String, ErrBox> {
  // todo :)
  Ok(text.to_string())
}
