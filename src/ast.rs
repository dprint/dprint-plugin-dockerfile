// AST types for the dockerfile parser.
//
// These mirror the subset of the `dockerfile-parser` crate's public types that
// the formatter relies on. They are produced by [`crate::parser`].

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

  /// Determines the 0-indexed line number and line-relative span of this span.
  pub fn relative_span(&self, dockerfile: &Dockerfile) -> (usize, Span) {
    let mut line_start_offset = 0;
    let mut lines = 0;
    for (i, c) in dockerfile.content.as_bytes().iter().enumerate() {
      if i == self.start {
        break;
      }
      if *c == b'\n' {
        lines += 1;
        line_start_offset = i + 1;
      }
    }

    let start = self.start - line_start_offset;
    let end = start + (self.end - self.start);
    (lines, Span { start, end })
  }
}

impl From<(usize, usize)> for Span {
  fn from(tup: (usize, usize)) -> Span {
    Span::new(tup.0, tup.1)
  }
}

/// A parsed Dockerfile.
#[derive(Debug, Clone, PartialEq)]
pub struct Dockerfile {
  /// The raw content of the Dockerfile.
  pub content: String,
  /// An ordered list of all parsed instructions.
  pub instructions: Vec<Instruction>,
  /// The line-continuation / escape character, from a `# escape=` directive
  /// (`\` by default).
  pub escape: char,
}

impl Dockerfile {
  /// Parses a Dockerfile from a string.
  pub fn parse(input: &str) -> Result<Dockerfile, monch::ParseErrorFailureError> {
    crate::parser::parse(input)
  }
}

/// A single Dockerfile instruction.
#[derive(Debug, PartialEq, Eq, Clone)]
pub enum Instruction {
  From(FromInstruction),
  Arg(ArgInstruction),
  Label(LabelInstruction),
  Run(RunInstruction),
  Entrypoint(EntrypointInstruction),
  Cmd(CmdInstruction),
  Copy(CopyInstruction),
  Env(EnvInstruction),
  Shell(ShellInstruction),
  Onbuild(OnbuildInstruction),
  Healthcheck(HealthcheckInstruction),
  Heredoc(HeredocInstruction),
  Misc(MiscInstruction),
  /// A line that could not be parsed as a known instruction. It is kept
  /// verbatim so a single malformed line never fails formatting of the file.
  Unknown(SpannedString),
}

impl Instruction {
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
#[derive(Debug, Eq, PartialEq, Ord, PartialOrd, Clone)]
pub struct SpannedString {
  pub span: Span,
  pub content: String,
}

/// A comment with a character span.
#[derive(Debug, Eq, PartialEq, Ord, PartialOrd, Clone)]
pub struct SpannedComment {
  pub span: Span,
  pub content: String,
}

/// A string array (ex. `["executable", "param1", "param2"]`).
#[derive(Debug, Eq, PartialEq, Ord, PartialOrd, Clone)]
pub struct StringArray {
  pub span: Span,
  pub elements: Vec<SpannedString>,
}

/// A component of a breakable string.
#[derive(Debug, Eq, PartialEq, Ord, PartialOrd, Clone)]
pub enum BreakableStringComponent {
  String(SpannedString),
  Comment(SpannedComment),
}

/// A Docker string that may be broken across several lines, separated by line
/// continuations (`\\\n`), and possibly intermixed with comments.
#[derive(Debug, Eq, PartialEq, Ord, PartialOrd, Clone)]
pub struct BreakableString {
  pub span: Span,
  pub components: Vec<BreakableStringComponent>,
}

/// A string that is either in shell form or exec form (`["a", "b"]`).
#[derive(Debug, PartialEq, Eq, Clone)]
pub enum ShellOrExecExpr {
  Shell(BreakableString),
  Exec(StringArray),
}

/// A Dockerfile `FROM` instruction.
#[derive(Debug, PartialEq, Eq, Clone)]
pub struct FromInstruction {
  pub span: Span,
  pub flags: Vec<FromFlag>,
  pub image: SpannedString,
  pub alias: Option<SpannedString>,
}

/// A key/value pair passed to a `FROM` instruction as a flag.
#[derive(Debug, PartialEq, Eq, Clone)]
pub struct FromFlag {
  pub span: Span,
  pub name: SpannedString,
  pub value: SpannedString,
}

/// A Dockerfile `ARG` instruction.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArgInstruction {
  pub span: Span,
  pub name: SpannedString,
  pub value: Option<SpannedString>,
}

/// A Dockerfile `LABEL` instruction. A single instruction may set many labels.
#[derive(Debug, PartialEq, Eq, Clone)]
pub struct LabelInstruction {
  pub span: Span,
  pub labels: Vec<Label>,
}

/// A single label key/value pair.
#[derive(Debug, PartialEq, Eq, Clone)]
pub struct Label {
  pub span: Span,
  pub name: SpannedString,
  pub value: SpannedString,
}

/// A Dockerfile `RUN` instruction.
#[derive(Debug, PartialEq, Eq, Clone)]
pub struct RunInstruction {
  pub span: Span,
  pub expr: ShellOrExecExpr,
}

/// A Dockerfile `ENTRYPOINT` instruction.
#[derive(Debug, PartialEq, Eq, Clone)]
pub struct EntrypointInstruction {
  pub span: Span,
  pub expr: ShellOrExecExpr,
}

/// A Dockerfile `CMD` instruction.
#[derive(Debug, PartialEq, Eq, Clone)]
pub struct CmdInstruction {
  pub span: Span,
  pub expr: ShellOrExecExpr,
}

/// A Dockerfile `COPY` instruction.
#[derive(Debug, PartialEq, Eq, Clone)]
pub struct CopyInstruction {
  pub span: Span,
  pub flags: Vec<CopyFlag>,
  pub args: CopyArgs,
}

/// The argument portion of a `COPY` instruction: either space-separated paths
/// or the JSON/exec array form (`COPY ["src", "dest"]`).
#[derive(Debug, PartialEq, Eq, Clone)]
pub enum CopyArgs {
  Paths { sources: Vec<SpannedString>, destination: SpannedString },
  Exec(StringArray),
}

/// A key/value pair passed to a `COPY` instruction as a flag.
#[derive(Debug, PartialEq, Eq, Clone)]
pub struct CopyFlag {
  pub span: Span,
  pub name: SpannedString,
  pub value: SpannedString,
}

/// A Dockerfile `ENV` instruction.
#[derive(Debug, PartialEq, Eq, Clone)]
pub struct EnvInstruction {
  pub span: Span,
  pub vars: Vec<EnvVar>,
}

/// An environment variable key/value pair.
#[derive(Debug, PartialEq, Eq, Clone)]
pub struct EnvVar {
  pub span: Span,
  pub key: SpannedString,
  pub value: BreakableString,
}

/// A Dockerfile `SHELL` instruction. Docker only permits the exec (JSON array)
/// form, but the shell form is tolerated so malformed input still round-trips.
#[derive(Debug, PartialEq, Eq, Clone)]
pub struct ShellInstruction {
  pub span: Span,
  pub expr: ShellOrExecExpr,
}

/// A Dockerfile `ONBUILD` instruction, wrapping the instruction it triggers.
#[derive(Debug, PartialEq, Eq, Clone)]
pub struct OnbuildInstruction {
  pub span: Span,
  pub instruction: Box<Instruction>,
}

/// A Dockerfile `HEALTHCHECK` instruction.
///
/// Either `HEALTHCHECK [OPTIONS] CMD <command>` (with `cmd` set to the nested
/// `CMD` instruction) or `HEALTHCHECK NONE` (with `cmd` being `None`).
#[derive(Debug, PartialEq, Eq, Clone)]
pub struct HealthcheckInstruction {
  pub span: Span,
  pub flags: Vec<HealthcheckFlag>,
  pub cmd: Option<Box<Instruction>>,
}

/// A key/value option passed to a `HEALTHCHECK` instruction, e.g.
/// `--interval=30s`.
#[derive(Debug, PartialEq, Eq, Clone)]
pub struct HealthcheckFlag {
  pub span: Span,
  pub name: SpannedString,
  pub value: SpannedString,
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
pub struct HeredocInstruction {
  pub span: Span,
  pub instruction: Box<Instruction>,
  pub body: String,
}

/// A miscellaneous (otherwise unsupported) Dockerfile instruction.
///
/// Includes valid-but-unparsed commands such as `EXPOSE`, `VOLUME`, `USER`,
/// `WORKDIR`, `ONBUILD`, `STOPSIGNAL`, `HEALTHCHECK`, `SHELL`, `MAINTAINER`.
#[derive(Debug, PartialEq, Eq, Clone)]
pub struct MiscInstruction {
  pub span: Span,
  pub instruction: SpannedString,
  pub arguments: BreakableString,
}
