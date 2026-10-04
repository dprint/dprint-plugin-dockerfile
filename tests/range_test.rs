use std::collections::HashSet;
use std::ops::Range;
use std::path::Path;
use std::path::PathBuf;

use dprint_core::configuration::*;
use dprint_development::*;
use dprint_plugin_dockerfile::configuration::Configuration;
use dprint_plugin_dockerfile::configuration::resolve_config;
use dprint_plugin_dockerfile::*;

/// Formats the cursor positions and line selections of every spec's text, and
/// ensures formatting a range never does something formatting the whole file
/// wouldn't have done.
#[test]
fn test_ranges_of_specs() {
  let mut file_paths = Vec::new();
  collect_spec_files(Path::new("./tests/specs"), &mut file_paths);
  assert!(!file_paths.is_empty());

  let mut failures = Vec::new();
  let mut checked_count = 0;
  for file_path in file_paths {
    let file_text = std::fs::read_to_string(&file_path).unwrap().replace("\r\n", "\n");
    let options = ParseSpecOptions {
      default_file_name: "file.dockerfile",
    };
    for spec in parse_specs(file_text, &options) {
      let spec_config: ConfigKeyMap = serde_json::from_value(spec.config.clone().into()).unwrap();
      let config_result = resolve_config(spec_config, &GlobalConfiguration::default());
      ensure_no_diagnostics(&config_result.diagnostics);
      let path = Path::new(&spec.file_name);
      let config = &config_result.config;
      let texts = [
        spec.file_text.clone(),
        spec.expected_text.clone(),
        spec.file_text.replace('\n', "\r\n"),
        format!("\u{FEFF}{}", spec.file_text),
        // other text following an instruction on its line
        spec.file_text.replace('\n', " # comment\n"),
        spec.file_text.replace('\n', " \\\n"),
      ];
      for text in texts {
        // only texts the formatter settles on in one go can be compared
        let formatted_file = format_file(path, &text, config);
        if format_file(path, &formatted_file, config) != formatted_file {
          continue;
        }
        checked_count += 1;
        // most ranges of a text give the same result
        let mut checked_results = HashSet::new();
        for range in ranges(&text) {
          let Some(result) = format_text_range(path, &text, range.clone(), config).unwrap() else {
            continue;
          };
          if !checked_results.insert(result.clone()) {
            continue;
          }
          if let Err(message) = check_result(path, &text, &result, &formatted_file, config) {
            failures.push(format!(
              "{} ({})\nRange: {:?}\nText: {:?}\nResult: {:?}\n{}",
              spec.message,
              file_path.display(),
              range,
              text,
              result,
              message
            ));
          }
        }
      }
    }
  }

  // guards against the check above skipping most of the texts
  assert!(checked_count > 500, "only checked {} texts", checked_count);
  if !failures.is_empty() {
    panic!(
      "{} failures. First failures:\n\n{}",
      failures.len(),
      failures[..failures.len().min(5)].join("\n\n")
    );
  }
}

fn check_result(path: &Path, text: &str, result: &str, formatted_file: &str, config: &Configuration) -> Result<(), String> {
  if formatted_file == text {
    return Err("Changed text that was already formatted.".to_string());
  }
  if result == formatted_file {
    return Ok(());
  }
  // only formatting the whole file changes the line endings or removes the bom
  let has_lone_line_feed = |text: &str| text.replace("\r\n", "").contains('\n');
  if !has_lone_line_feed(text) && has_lone_line_feed(result) || !text.contains('\r') && result.contains('\r') {
    return Err("Changed the line endings.".to_string());
  }
  if text.starts_with('\u{FEFF}') != result.starts_with('\u{FEFF}') {
    return Err("Changed the bom.".to_string());
  }
  let formatted_result = format_file(path, result, config);
  if normalize_new_lines(&formatted_result) != normalize_new_lines(formatted_file) {
    return Err(format!(
      "Formatting the file after the range didn't give the same text as formatting the file.\nAfter range: {:?}\nFile: {:?}",
      formatted_result, formatted_file
    ));
  }
  Ok(())
}

/// Every cursor position along with the selections of whole and partial lines.
fn ranges(text: &str) -> Vec<Range<usize>> {
  let positions = (0..=text.len()).filter(|pos| text.is_char_boundary(*pos)).collect::<Vec<_>>();
  let mut ranges = positions.iter().map(|pos| *pos..*pos).collect::<Vec<_>>();
  let mut line_positions = vec![0];
  for (index, _) in text.match_indices('\n') {
    line_positions.extend([index.saturating_sub(1), index, index + 1, (index + 3).min(text.len())]);
  }
  line_positions.push(text.len());
  line_positions.retain(|pos| text.is_char_boundary(*pos));
  line_positions.sort();
  line_positions.dedup();
  for (i, start) in line_positions.iter().enumerate() {
    for end in &line_positions[i + 1..] {
      ranges.push(*start..*end);
    }
  }
  ranges
}

fn format_file(path: &Path, text: &str, config: &Configuration) -> String {
  format_text(path, text, config).unwrap().unwrap_or_else(|| text.to_string())
}

fn normalize_new_lines(text: &str) -> String {
  text.replace("\r\n", "\n")
}

fn collect_spec_files(dir: &Path, file_paths: &mut Vec<PathBuf>) {
  for entry in std::fs::read_dir(dir).unwrap() {
    let path = entry.unwrap().path();
    if path.is_dir() {
      collect_spec_files(&path, file_paths);
    } else {
      file_paths.push(path);
    }
  }
}
