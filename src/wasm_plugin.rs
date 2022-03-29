use dprint_core::configuration::ConfigKeyMap;
use dprint_core::configuration::GlobalConfiguration;
use dprint_core::configuration::ResolveConfigurationResult;
use dprint_core::generate_plugin_code;
use dprint_core::plugins::FormatResult;
use dprint_core::plugins::PluginInfo;
use dprint_core::plugins::SyncPluginHandler;
use std::path::Path;

use super::configuration::resolve_config;
use super::configuration::Configuration;

struct DockerfilePluginHandler;

impl SyncPluginHandler<Configuration> for DockerfilePluginHandler {
  fn resolve_config(&mut self, config: ConfigKeyMap, global_config: &GlobalConfiguration) -> ResolveConfigurationResult<Configuration> {
    resolve_config(config, global_config)
  }

  fn plugin_info(&mut self) -> PluginInfo {
    let version = env!("CARGO_PKG_VERSION").to_string();
    PluginInfo {
      name: env!("CARGO_PKG_NAME").to_string(),
      version: version.clone(),
      config_key: "dockerfile".to_string(),
      file_extensions: vec!["dockerfile".to_string()],
      file_names: vec!["Dockerfile".to_string()],
      help_url: "https://dprint.dev/plugins/dockerfile".to_string(),
      config_schema_url: format!("https://plugins.dprint.dev/dprint/dprint-plugin-dockerfile/{}/schema.json", version),
      update_url: Some("https://plugins.dprint.dev/dprint/dprint-plugin-dockerfile/latest.json".to_string()),
    }
  }

  fn license_text(&mut self) -> String {
    std::str::from_utf8(include_bytes!("../LICENSE")).unwrap().into()
  }

  fn format(
    &mut self,
    file_path: &Path,
    file_text: &str,
    config: &Configuration,
    _format_with_host: impl FnMut(&Path, String, &ConfigKeyMap) -> FormatResult,
  ) -> FormatResult {
    super::format_text(file_path, file_text, config)
  }
}

generate_plugin_code!(DockerfilePluginHandler, DockerfilePluginHandler);
