// AST types for the dockerfile parser.
//
// These mirror the subset of the `dockerfile-parser` crate's public types that
// the formatter relies on. They are produced by [`crate::parser`] and borrow
// from the original Dockerfile text, so parsing only allocates for the few
// strings whose content differs from the source (escaped or quoted values).

use std::borrow::Cow;

/// A byte-index range into the original Dockerfile text.
#[derive(PartialEq, Eq, Clone, Copy, Debug, Ord, PartialOrd)]
pub struct Span {
  pub start: usize,
  pub end: usize,
}

impl Span {
  pub fn new(start: usize, end: usize) -> Span {
    Span { start, end }
  }
}

impl From<(usize, usize)> for Span {
  fn from(tup: (usize, usize)) -> Span {
    Span::new(tup.0, tup.1)
  }
}

/// A parsed Dockerfile.
#[derive(Debug, Clone, PartialEq)]
pub struct Dockerfile<'a> {
  /// The raw content of the Dockerfile.
  pub content: &'a str,
  /// An ordered list of all parsed instructions.
  pub instructions: Vec<Instruction<'a>>,
  /// The line-continuation / escape character, from a `# escape=` directive
  /// (`\` by default).
  pub escape: char,
  /// The byte offset each line of `content` starts at, in ascending order and
  /// always beginning with `0`. Kept private because it is derived from
  /// `content` and must not fall out of sync with it.
  line_starts: Vec<usize>,
}

impl<'a> Dockerfile<'a> {
  /// Parses a Dockerfile from a string.
  pub fn parse(input: &'a str) -> Result<Dockerfile<'a>, monch::ParseErrorFailureError> {
    crate::parser::parse(input)
  }

  pub fn new(content: &'a str, instructions: Vec<Instruction<'a>>, escape: char) -> Dockerfile<'a> {
    Dockerfile {
      content,
      instructions,
      escape,
      line_starts: line_starts(content),
    }
  }

  /// The 0-indexed number of the line the given byte position is on.
  ///
  /// Resolved against the precomputed line table, so this stays cheap on a
  /// large file where scanning back to the start of the text would not.
  pub fn line_index(&self, pos: usize) -> usize {
    match self.line_starts.binary_search(&pos) {
      Ok(index) => index,
      // `line_starts` always starts at 0, so an insertion point is never 0
      Err(index) => index - 1,
    }
  }

  /// The byte offset the given 0-indexed line starts at.
  pub fn line_start(&self, line_index: usize) -> usize {
    self.line_starts[line_index]
  }
}

/// The byte offset each line of `content` starts at. A `\r\n` line ending is
/// covered by its `\n`, so both line ending styles are handled.
fn line_starts(content: &str) -> Vec<usize> {
  let mut starts = vec![0];
  starts.extend(content.bytes().enumerate().filter(|(_, b)| *b == b'\n').map(|(i, _)| i + 1));
  starts
}

/// A single Dockerfile instruction.
#[derive(Debug, PartialEq, Eq, Clone)]
pub enum Instruction<'a> {
  From(FromInstruction<'a>),
  Arg(ArgInstruction<'a>),
  Label(LabelInstruction<'a>),
  Run(RunInstruction<'a>),
  Entrypoint(EntrypointInstruction<'a>),
  Cmd(CmdInstruction<'a>),
  Copy(CopyInstruction<'a>),
  Env(EnvInstruction<'a>),
  Shell(ShellInstruction<'a>),
  Onbuild(OnbuildInstruction<'a>),
  Healthcheck(HealthcheckInstruction<'a>),
  Heredoc(HeredocInstruction<'a>),
  Misc(MiscInstruction<'a>),
  /// A line that could not be parsed as a known instruction. It is kept
  /// verbatim so a single malformed line never fails formatting of the file.
  Unknown(SpannedString<'a>),
}

impl Instruction<'_> {
  pub fn span(&self) -> Span {
    match self {
      Instruction::From(i) => i.span,
      Instruction::Arg(i) => i.span,
      Instruction::Label(i) => i.span,
      Instruction::Run(i) => i.span,
      Instruction::Entrypoint(i) => i.span,
      Instruction::Cmd(i) => i.span,
      Instruction::Copy(i) => i.span,
      Instruction::Env(i) => i.span,
      Instruction::Shell(i) => i.span,
      Instruction::Onbuild(i) => i.span,
      Instruction::Healthcheck(i) => i.span,
      Instruction::Heredoc(i) => i.span,
      Instruction::Misc(i) => i.span,
      Instruction::Unknown(i) => i.span,
    }
  }
}

/// A string with a character span.
///
/// The content borrows from the original text unless the parser had to resolve
/// escape sequences or strip quotes, in which case it is owned.
#[derive(Debug, Eq, PartialEq, Ord, PartialOrd, Clone)]
pub struct SpannedString<'a> {
  pub span: Span,
  pub content: Cow<'a, str>,
}

/// A comment with a character span.
#[derive(Debug, Eq, PartialEq, Ord, PartialOrd, Clone)]
pub struct SpannedComment<'a> {
  pub span: Span,
  pub content: &'a str,
}

/// A string array (ex. `["executable", "param1", "param2"]`).
#[derive(Debug, Eq, PartialEq, Ord, PartialOrd, Clone)]
pub struct StringArray<'a> {
  pub span: Span,
  pub elements: Vec<SpannedString<'a>>,
}

/// A component of a breakable string.
#[derive(Debug, Eq, PartialEq, Ord, PartialOrd, Clone)]
pub enum BreakableStringComponent<'a> {
  String(SpannedString<'a>),
  Comment(SpannedComment<'a>),
}

/// A Docker string that may be broken across several lines, separated by line
/// continuations (`\\\n`), and possibly intermixed with comments.
#[derive(Debug, Eq, PartialEq, Ord, PartialOrd, Clone)]
pub struct BreakableString<'a> {
  pub span: Span,
  pub components: Vec<BreakableStringComponent<'a>>,
}

/// A string that is either in shell form or exec form (`["a", "b"]`).
#[derive(Debug, PartialEq, Eq, Clone)]
pub enum ShellOrExecExpr<'a> {
  Shell(BreakableString<'a>),
  Exec(StringArray<'a>),
}

/// A Dockerfile `FROM` instruction.
#[derive(Debug, PartialEq, Eq, Clone)]
pub struct FromInstruction<'a> {
  pub span: Span,
  pub flags: Vec<FromFlag<'a>>,
  pub image: SpannedString<'a>,
  pub alias: Option<SpannedString<'a>>,
}

/// A key/value pair passed to a `FROM` instruction as a flag.
#[derive(Debug, PartialEq, Eq, Clone)]
pub struct FromFlag<'a> {
  pub span: Span,
  pub name: SpannedString<'a>,
  pub value: SpannedString<'a>,
}

/// A Dockerfile `ARG` instruction.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArgInstruction<'a> {
  pub span: Span,
  pub name: SpannedString<'a>,
  pub value: Option<SpannedString<'a>>,
}

/// A Dockerfile `LABEL` instruction. A single instruction may set many labels.
#[derive(Debug, PartialEq, Eq, Clone)]
pub struct LabelInstruction<'a> {
  pub span: Span,
  pub labels: Vec<Label<'a>>,
}

/// A single label key/value pair.
#[derive(Debug, PartialEq, Eq, Clone)]
pub struct Label<'a> {
  pub span: Span,
  pub name: SpannedString<'a>,
  pub value: SpannedString<'a>,
}

/// A Dockerfile `RUN` instruction.
#[derive(Debug, PartialEq, Eq, Clone)]
pub struct RunInstruction<'a> {
  pub span: Span,
  pub expr: ShellOrExecExpr<'a>,
}

/// A Dockerfile `ENTRYPOINT` instruction.
#[derive(Debug, PartialEq, Eq, Clone)]
pub struct EntrypointInstruction<'a> {
  pub span: Span,
  pub expr: ShellOrExecExpr<'a>,
}

/// A Dockerfile `CMD` instruction.
#[derive(Debug, PartialEq, Eq, Clone)]
pub struct CmdInstruction<'a> {
  pub span: Span,
  pub expr: ShellOrExecExpr<'a>,
}

/// A Dockerfile `COPY` instruction.
#[derive(Debug, PartialEq, Eq, Clone)]
pub struct CopyInstruction<'a> {
  pub span: Span,
  pub flags: Vec<CopyFlag<'a>>,
  pub args: CopyArgs<'a>,
}

/// The argument portion of a `COPY` instruction: either space-separated paths
/// or the JSON/exec array form (`COPY ["src", "dest"]`).
#[derive(Debug, PartialEq, Eq, Clone)]
pub enum CopyArgs<'a> {
  Paths {
    sources: Vec<SpannedString<'a>>,
    destination: SpannedString<'a>,
  },
  Exec(StringArray<'a>),
}

/// A key/value pair passed to a `COPY` instruction as a flag.
#[derive(Debug, PartialEq, Eq, Clone)]
pub struct CopyFlag<'a> {
  pub span: Span,
  pub name: SpannedString<'a>,
  pub value: SpannedString<'a>,
}

/// A Dockerfile `ENV` instruction.
#[derive(Debug, PartialEq, Eq, Clone)]
pub struct EnvInstruction<'a> {
  pub span: Span,
  pub vars: Vec<EnvVar<'a>>,
}

/// An environment variable key/value pair.
#[derive(Debug, PartialEq, Eq, Clone)]
pub struct EnvVar<'a> {
  pub span: Span,
  pub key: SpannedString<'a>,
  pub value: BreakableString<'a>,
}

/// A Dockerfile `SHELL` instruction. Docker only permits the exec (JSON array)
/// form, but the shell form is tolerated so malformed input still round-trips.
#[derive(Debug, PartialEq, Eq, Clone)]
pub struct ShellInstruction<'a> {
  pub span: Span,
  pub expr: ShellOrExecExpr<'a>,
}

/// A Dockerfile `ONBUILD` instruction, wrapping the instruction it triggers.
#[derive(Debug, PartialEq, Eq, Clone)]
pub struct OnbuildInstruction<'a> {
  pub span: Span,
  pub instruction: Box<Instruction<'a>>,
}

/// A Dockerfile `HEALTHCHECK` instruction.
///
/// Either `HEALTHCHECK [OPTIONS] CMD <command>` (with `cmd` set to the nested
/// `CMD` instruction) or `HEALTHCHECK NONE` (with `cmd` being `None`).
#[derive(Debug, PartialEq, Eq, Clone)]
pub struct HealthcheckInstruction<'a> {
  pub span: Span,
  pub flags: Vec<HealthcheckFlag<'a>>,
  pub cmd: Option<Box<Instruction<'a>>>,
}

/// A key/value option passed to a `HEALTHCHECK` instruction, e.g.
/// `--interval=30s`.
#[derive(Debug, PartialEq, Eq, Clone)]
pub struct HealthcheckFlag<'a> {
  pub span: Span,
  pub name: SpannedString<'a>,
  pub value: SpannedString<'a>,
}

/// An instruction that carries one or more [heredocs][heredoc], e.g.
///
/// ```dockerfile
/// RUN <<EOF
/// echo hello
/// EOF
/// ```
///
/// The first line is the wrapped `instruction` (parsed normally, so it still
/// gets formatted); `body` is the verbatim text of the heredoc bodies and their
/// closing delimiters, preserved exactly.
///
/// [heredoc]: https://docs.docker.com/engine/reference/builder/#here-documents
#[derive(Debug, PartialEq, Eq, Clone)]
pub struct HeredocInstruction<'a> {
  pub span: Span,
  pub instruction: Box<Instruction<'a>>,
  pub body: &'a str,
}

/// A miscellaneous (otherwise unsupported) Dockerfile instruction.
///
/// Includes valid-but-unparsed commands such as `EXPOSE`, `VOLUME`, `USER`,
/// `WORKDIR`, `ONBUILD`, `STOPSIGNAL`, `HEALTHCHECK`, `SHELL`, `MAINTAINER`.
#[derive(Debug, PartialEq, Eq, Clone)]
pub struct MiscInstruction<'a> {
  pub span: Span,
  pub instruction: SpannedString<'a>,
  pub arguments: BreakableString<'a>,
}

#[cfg(test)]
mod test {
  use super::*;

  #[test]
  fn resolves_line_indexes() {
    let file = Dockerfile::new("FROM a\nRUN b\n\nCMD c", Vec::new(), '\\');
    // the start of a line, a position within it, and its own newline all
    // resolve to that line; the blank line 2 is just the newline at 13
    let cases = [(0, 0), (3, 0), (6, 0), (7, 1), (11, 1), (12, 1), (13, 2), (14, 3), (18, 3)];
    for (pos, expected) in cases {
      assert_eq!(file.line_index(pos), expected, "pos {pos}");
    }
    // the end of the text belongs to the last line
    assert_eq!(file.line_index(file.content.len()), 3);
    assert_eq!(file.line_start(3), 14);
  }

  #[test]
  fn resolves_line_indexes_with_carriage_returns() {
    // the `\r` of a `\r\n` ending is still part of the line it terminates
    let file = Dockerfile::new("FROM a\r\nRUN b\r\n", Vec::new(), '\\');
    assert_eq!(file.line_index(6), 0);
    assert_eq!(file.line_index(7), 0);
    assert_eq!(file.line_index(8), 1);
    assert_eq!(file.line_start(1), 8);
  }

  #[test]
  fn resolves_line_indexes_in_empty_text() {
    let file = Dockerfile::new("", Vec::new(), '\\');
    assert_eq!(file.line_index(0), 0);
    assert_eq!(file.line_start(0), 0);
  }
}
