use dprint_core::configuration::ConfigKeyMap;
use dprint_core::configuration::GlobalConfiguration;
use dprint_core::generate_plugin_code;
use dprint_core::plugins::CheckConfigUpdatesMessage;
use dprint_core::plugins::ConfigChange;
use dprint_core::plugins::FileMatchingInfo;
use dprint_core::plugins::FormatError;
use dprint_core::plugins::FormatResult;
use dprint_core::plugins::PluginInfo;
use dprint_core::plugins::PluginResolveConfigurationResult;
use dprint_core::plugins::SyncFormatRequest;
use dprint_core::plugins::SyncHostFormatRequest;
use dprint_core::plugins::SyncPluginHandler;

use super::configuration::Configuration;
use super::configuration::resolve_config;

struct DockerfilePluginHandler;

impl SyncPluginHandler<Configuration> for DockerfilePluginHandler {
  fn resolve_config(&mut self, config: ConfigKeyMap, global_config: &GlobalConfiguration) -> PluginResolveConfigurationResult<Configuration> {
    let result = resolve_config(config, global_config);
    PluginResolveConfigurationResult {
      config: result.config,
      diagnostics: result.diagnostics,
      file_matching: FileMatchingInfo {
        file_extensions: vec!["dockerfile".to_string()],
        file_names: vec!["Dockerfile".to_string()],
      },
    }
  }

  fn plugin_info(&mut self) -> PluginInfo {
    let version = env!("CARGO_PKG_VERSION").to_string();
    PluginInfo {
      name: env!("CARGO_PKG_NAME").to_string(),
      version: version.clone(),
      config_key: "dockerfile".to_string(),
      help_url: "https://dprint.dev/plugins/dockerfile".to_string(),
      config_schema_url: format!("https://plugins.dprint.dev/dprint/dprint-plugin-dockerfile/{}/schema.json", version),
      update_url: Some("https://plugins.dprint.dev/dprint/dprint-plugin-dockerfile/latest.json".to_string()),
    }
  }

  fn check_config_updates(&self, _message: CheckConfigUpdatesMessage) -> Result<Vec<ConfigChange>, FormatError> {
    Ok(Vec::new())
  }

  fn license_text(&mut self) -> String {
    std::str::from_utf8(include_bytes!("../LICENSE")).unwrap().into()
  }

  fn format(&mut self, request: SyncFormatRequest<Configuration>, _format_with_host: impl FnMut(SyncHostFormatRequest) -> FormatResult) -> FormatResult {
    let file_text = String::from_utf8(request.file_bytes)?;
    let result = super::format_text(request.file_path, &file_text, request.config).map_err(FormatError::new)?;
    Ok(result.map(|file_text| file_text.into_bytes()))
  }
}

generate_plugin_code!(DockerfilePluginHandler, DockerfilePluginHandler);
