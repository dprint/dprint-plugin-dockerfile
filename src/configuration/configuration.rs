use dprint_core::configuration::NewLineKind;
use serde::Deserialize;
use serde::Serialize;

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Configuration {
  pub line_width: u32,
  pub new_line_kind: NewLineKind,
  /// Whether to always break a `HEALTHCHECK` command onto its own continuation
  /// line when the instruction has options, even if it would fit on one line.
  pub healthcheck_cmd_new_line: bool,
  /// Whether to indent the instructions that follow a `FROM` instruction.
  pub indent_stages: bool,
  /// The number of spaces used for a level of indentation.
  pub indent_width: u8,
}
