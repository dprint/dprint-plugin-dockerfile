use dprint_core::configuration::ConfigKeyMap;
use dprint_core::configuration::ConfigKeyValue;
use dprint_core::configuration::GlobalConfiguration;
use dprint_core::configuration::NewLineKind;

use super::*;

/// Formatting configuration builder.
///
/// # Example
///
/// ```
/// use dprint_plugin_dockerfile::configuration::*;
///
/// let config = ConfigurationBuilder::new()
///     .line_width(80)
///     .build();
/// ```
pub struct ConfigurationBuilder {
  pub(super) config: ConfigKeyMap,
  global_config: Option<GlobalConfiguration>,
}

impl Default for ConfigurationBuilder {
  fn default() -> Self {
    Self::new()
  }
}

impl ConfigurationBuilder {
  /// Constructs a new configuration builder.
  pub fn new() -> ConfigurationBuilder {
    ConfigurationBuilder {
      config: ConfigKeyMap::new(),
      global_config: None,
    }
  }

  /// Gets the final configuration that can be used to format a file.
  pub fn build(&self) -> Configuration {
    if let Some(global_config) = &self.global_config {
      resolve_config(self.config.clone(), global_config).config
    } else {
      let global_config = GlobalConfiguration::default();
      resolve_config(self.config.clone(), &global_config).config
    }
  }

  /// Set the global configuration.
  pub fn global_config(&mut self, global_config: GlobalConfiguration) -> &mut Self {
    self.global_config = Some(global_config);
    self
  }

  /// The width of a line the printer will try to stay under. Note that the printer may exceed this width in certain cases.
  /// Default: 120
  pub fn line_width(&mut self, value: u32) -> &mut Self {
    self.insert("lineWidth", (value as i32).into())
  }

  /// The kind of newline to use.
  /// Default: `NewLineKind::LineFeed`
  pub fn new_line_kind(&mut self, value: NewLineKind) -> &mut Self {
    self.insert("newLineKind", value.to_string().into())
  }

  /// Whether to always break a `HEALTHCHECK` command onto its own continuation
  /// line when the instruction has options, even if it would fit on one line.
  /// Default: `false`
  pub fn healthcheck_cmd_new_line(&mut self, value: bool) -> &mut Self {
    self.insert("healthcheckCmdNewLine", value.into())
  }

  /// Whether to indent the body of each build stage - the instructions and
  /// comments that follow a `FROM` - by `indent_width` spaces. The indentation
  /// is always spaces; `useTabs` is not honored.
  /// Default: `false`
  pub fn indent_stages(&mut self, value: bool) -> &mut Self {
    self.insert("indentStages", value.into())
  }

  /// The number of spaces for an indent. Only used when `indent_stages` is
  /// enabled.
  /// Default: 2
  pub fn indent_width(&mut self, value: u8) -> &mut Self {
    self.insert("indentWidth", (value as i32).into())
  }

  #[cfg(test)]
  pub(super) fn get_inner_config(&self) -> ConfigKeyMap {
    self.config.clone()
  }

  fn insert(&mut self, name: &str, value: ConfigKeyValue) -> &mut Self {
    self.config.insert(String::from(name), value);
    self
  }
}

#[cfg(test)]
mod tests {
  use dprint_core::configuration::NewLineKind;
  use dprint_core::configuration::resolve_global_config;

  use super::*;

  #[test]
  fn check_all_values_set() {
    let mut config = ConfigurationBuilder::new();
    config
      .new_line_kind(NewLineKind::CarriageReturnLineFeed)
      .line_width(90)
      .healthcheck_cmd_new_line(true)
      .indent_stages(true)
      .indent_width(4);

    let inner_config = config.get_inner_config();
    assert_eq!(inner_config.len(), 5);
    let diagnostics = resolve_config(inner_config, &Default::default()).diagnostics;
    assert_eq!(diagnostics.len(), 0);
  }

  #[test]
  fn handle_global_config() {
    let mut global_config = ConfigKeyMap::new();
    global_config.insert(String::from("lineWidth"), 90.into());
    global_config.insert(String::from("newLineKind"), "crlf".into());
    global_config.insert(String::from("indentWidth"), 4.into());
    global_config.insert(String::from("useTabs"), true.into());
    let global_config = resolve_global_config(&mut global_config).config;
    let mut config_builder = ConfigurationBuilder::new();
    let config = config_builder.global_config(global_config).build();
    assert_eq!(config.line_width, 90);
    assert_eq!(config.new_line_kind, NewLineKind::CarriageReturnLineFeed);
    assert_eq!(config.indent_width, 4);
  }

  #[test]
  fn use_defaults_when_global_not_set() {
    let global_config = Default::default();
    let mut config_builder = ConfigurationBuilder::new();
    let config = config_builder.global_config(global_config).build();
    assert_eq!(config.new_line_kind, NewLineKind::LineFeed);
    assert_eq!(config.indent_width, 2);
    assert!(!config.indent_stages);
  }

  #[test]
  fn override_global_indent_width() {
    let mut global_config = ConfigKeyMap::new();
    global_config.insert(String::from("indentWidth"), 4.into());
    let global_config = resolve_global_config(&mut global_config).config;
    let mut config_builder = ConfigurationBuilder::new();
    let config = config_builder.global_config(global_config).indent_width(8).build();
    assert_eq!(config.indent_width, 8);
  }
}
