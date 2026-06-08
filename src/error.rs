pub use monch::ParseErrorFailureError as ParseError;

/// An error that can occur while formatting a Dockerfile.
#[derive(Debug, thiserror::Error)]
pub enum FormatError {
  /// The input could not be parsed as a Dockerfile.
  #[error(transparent)]
  Parse(#[from] ParseError),
}
