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
  /// Whether to indent the body of each build stage - the instructions and
  /// comments that follow a `FROM` - by `indent_width` spaces. The indentation
  /// is always spaces; `useTabs` is not honored.
  pub indent_stages: bool,
  /// The number of spaces for an indent. Only used when `indent_stages` is
  /// enabled.
  pub indent_width: u8,
}
