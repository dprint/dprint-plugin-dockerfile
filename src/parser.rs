// A dockerfile parser built on the `monch` parser combinator library.
//
// The parser closely follows the grammar of the `dockerfile-parser` crate so
// that the formatter can rely on the same spans and structure. Byte spans are
// recovered from `&str` subslices via pointer arithmetic against the original
// input (see [`Parser::off`]).

use monch::*;

use crate::ast::*;

type PResult<'a, T> = Result<(&'a str, T), ParseErrorFailureError>;

/// Parses a Dockerfile from a string.
pub fn parse(text: &str) -> Result<Dockerfile, ParseErrorFailureError> {
  let parser = Parser { base: text };
  let instructions = parser.parse_dockerfile(text)?;
  Ok(Dockerfile {
    content: text.to_string(),
    instructions,
  })
}

/// The set of instruction keywords that have dedicated parsing. Everything else
/// is parsed as a [`MiscInstruction`].
const KEYWORDS: [&str; 11] = [
  "from",
  "run",
  "arg",
  "label",
  "copy",
  "entrypoint",
  "cmd",
  "env",
  "shell",
  "onbuild",
  "healthcheck",
];

struct Parser<'a> {
  base: &'a str,
}

impl<'a> Parser<'a> {
  fn parse_dockerfile(&self, mut input: &'a str) -> Result<Vec<Instruction>, ParseErrorFailureError> {
    let mut instructions = Vec::new();
    loop {
      // a meta step starts with optional insignificant whitespace
      let after_ws = skip_ws(input);
      if after_ws.is_empty() {
        break;
      }
      if let Some(rest) = strip_newline(after_ws) {
        // empty line
        input = rest;
        continue;
      }
      if after_ws.starts_with('#') {
        // a standalone comment line — the formatter re-discovers these by
        // scanning the gaps between instruction spans, so we just skip them
        input = skip_to_next_line(after_ws);
        continue;
      }

      let (rest, instruction) = self.parse_instruction(after_ws)?;
      let (rest, instruction) = self.maybe_consume_heredocs(instruction, rest);
      instructions.push(instruction);
      input = self.finish_line(rest)?;
    }
    Ok(instructions)
  }

  fn parse_instruction(&self, input: &'a str) -> PResult<'a, Instruction> {
    let (after_kw, keyword) = alpha0(input);
    let lower = keyword.to_ascii_lowercase();
    if KEYWORDS.contains(&lower.as_str()) {
      // a known keyword is only honored if followed by argument whitespace,
      // otherwise it falls through to a misc instruction (matching the grammar)
      if let Some(after_arg_ws) = self.arg_ws(after_kw) {
        let start = self.off(input);
        // shell-form instructions keep a leading comment in their breakable
        // string rather than discarding it (see #12)
        let shell_start = self.arg_ws_keep_comments(after_kw).unwrap_or(after_arg_ws);
        return match lower.as_str() {
          "from" => self.parse_from(after_arg_ws, start),
          "run" => self.parse_shell_or_exec(shell_start, start, ExprKind::Run),
          "cmd" => self.parse_shell_or_exec(shell_start, start, ExprKind::Cmd),
          "entrypoint" => self.parse_shell_or_exec(shell_start, start, ExprKind::Entrypoint),
          "arg" => self.parse_arg(after_arg_ws, start),
          "label" => self.parse_label(after_kw, start),
          "copy" => self.parse_copy(after_kw, start),
          "env" => self.parse_env(after_kw, start),
          "shell" => self.parse_shell_or_exec(shell_start, start, ExprKind::Shell),
          // these wrap or normalize nested content; fall back to a misc
          // instruction if the structured form doesn't parse
          "onbuild" => self.parse_onbuild(after_arg_ws, start).or_else(|_| self.parse_misc(input)),
          "healthcheck" => self.parse_healthcheck(after_arg_ws, start).or_else(|_| self.parse_misc(input)),
          _ => unreachable!(),
        };
      }
    }
    self.parse_misc(input)
  }

  fn parse_from(&self, input: &'a str, start: usize) -> PResult<'a, Instruction> {
    let mut flags = Vec::new();
    let mut input = input;

    // (arg_ws ~ from_flag)* — the first arg_ws was already consumed before the
    // first token, so peek a flag and only continue while flags are present
    while let Some((rest, name, value, span)) = self.parse_flag(input) {
      flags.push(FromFlag { span, name, value });
      match self.arg_ws(rest) {
        Some(next) => input = next,
        None => {
          input = rest;
          break;
        }
      }
    }

    let (after_image, image) = self.parse_image(input)?;
    let mut end = after_image;
    let mut alias = None;
    if let Some((rest, value)) = self.parse_alias(after_image) {
      alias = Some(value);
      end = rest;
    }

    let span = Span::new(start, self.off(end));
    Ok((end, Instruction::From(FromInstruction { span, flags, image, alias })))
  }

  fn parse_image(&self, input: &'a str) -> PResult<'a, SpannedString> {
    let image = take_while(|c: char| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.' | ':' | '/' | '$' | '{' | '}' | '@'));
    match if_not_empty(image)(input) {
      Ok((rest, text)) => Ok((rest, self.spanned(input, rest, text.to_string()))),
      Err(_) => Err(fail("missing from image")),
    }
  }

  fn parse_alias(&self, after_image: &'a str) -> Option<(&'a str, SpannedString)> {
    // from_alias_outer = arg_ws ~ ^"as" ~ arg_ws ~ from_alias
    let after_ws = self.arg_ws(after_image)?;
    let after_as = strip_prefix_ci(after_ws, "as")?;
    let alias_start = self.arg_ws(after_as)?;
    let alias = take_while(|c: char| c.is_ascii_alphanumeric() || matches!(c, '_' | '-'));
    let (rest, text) = if_not_empty(alias)(alias_start).ok()?;
    Some((rest, self.spanned(alias_start, rest, text.to_string())))
  }

  fn parse_shell_or_exec(&self, input: &'a str, start: usize, kind: ExprKind) -> PResult<'a, Instruction> {
    let (rest, span, expr) = if input.starts_with('[') {
      match self.string_array(input) {
        Ok((rest, array)) => {
          let span = Span::new(start, array.span.end);
          (rest, span, ShellOrExecExpr::Exec(array))
        }
        // not a valid exec form — fall back to shell form
        Err(_) => self.shell_expr(input, start)?,
      }
    } else {
      self.shell_expr(input, start)?
    };
    Ok((rest, kind.build(span, expr)))
  }

  fn shell_expr(&self, input: &'a str, start: usize) -> Result<(&'a str, Span, ShellOrExecExpr), ParseErrorFailureError> {
    let (rest, breakable) = self.any_breakable(input)?;
    let span = Span::new(start, breakable.span.end);
    Ok((rest, span, ShellOrExecExpr::Shell(breakable)))
  }

  fn parse_arg(&self, input: &'a str, start: usize) -> PResult<'a, Instruction> {
    let (after_name, name) = self.arg_name(input)?;
    let mut end = after_name;
    let mut value = None;
    if let Some(after_eq) = after_name.strip_prefix('=') {
      let (rest, v) = self.value_quoted_or(after_eq, |p, s| p.any_whitespace(s))?;
      value = Some(v);
      end = rest;
    }
    let span = Span::new(start, self.off(end));
    Ok((end, Instruction::Arg(ArgInstruction { span, name, value })))
  }

  fn arg_name(&self, input: &'a str) -> PResult<'a, SpannedString> {
    // ASCII_ALPHA ~ (ASCII_ALPHANUMERIC | "_")*
    let name = substring(pair(
      if_true(next_char, |c| c.is_ascii_alphabetic()),
      skip_while(|c| c.is_ascii_alphanumeric() || c == '_'),
    ));
    match name(input) {
      Ok((rest, text)) => Ok((rest, self.spanned(input, rest, text.to_string()))),
      Err(_) => Err(fail("arg name is required")),
    }
  }

  fn parse_copy(&self, input: &'a str, start: usize) -> PResult<'a, Instruction> {
    let mut flags = Vec::new();
    let mut paths: Vec<SpannedString> = Vec::new();
    let mut input = input;

    // (arg_ws ~ copy_flag)*
    while let Some(after_ws) = self.arg_ws(input) {
      match self.parse_flag(after_ws) {
        Some((rest, name, value, span)) => {
          flags.push(CopyFlag { span, name, value });
          input = rest;
        }
        // leave whitespace unconsumed for the pathspec/exec loop
        None => break,
      }
    }

    // the exec (JSON array) form: `COPY ["src", "dest"]`
    if let Some(after_ws) = self.arg_ws(input)
      && after_ws.starts_with('[')
        && let Ok((rest, array)) = self.string_array(after_ws) {
          let span = Span::new(start, array.span.end);
          return Ok((
            rest,
            Instruction::Copy(CopyInstruction {
              span,
              flags,
              args: CopyArgs::Exec(array),
            }),
          ));
        }

    // (arg_ws ~ copy_pathspec){2,}
    while let Some(after_ws) = self.arg_ws(input) {
      match self.any_whitespace(after_ws) {
        Ok((rest, text)) => {
          paths.push(self.spanned(after_ws, rest, text.to_string()));
          input = rest;
        }
        Err(_) => break,
      }
    }

    if paths.len() < 2 {
      return Err(fail("copy requires at least one source and a destination"));
    }
    let destination = paths.pop().unwrap();
    let span = Span::new(start, destination.span.end);
    Ok((
      input,
      Instruction::Copy(CopyInstruction {
        span,
        flags,
        args: CopyArgs::Paths { sources: paths, destination },
      }),
    ))
  }

  fn parse_label(&self, after_kw: &'a str, start: usize) -> PResult<'a, Instruction> {
    let mut labels = Vec::new();
    let input;

    if let Some((rest, label)) = self.label_single(after_kw) {
      labels.push(label);
      input = rest;
    } else {
      // (arg_ws ~ label_pair?)+
      let mut current = after_kw;
      while let Some(after_ws) = self.arg_ws(current) {
        match self.label_pair(after_ws) {
          Some((rest, label)) => {
            labels.push(label);
            current = rest;
          }
          None => {
            current = after_ws;
            break;
          }
        }
      }
      input = current;
      if labels.is_empty() {
        return Err(fail("label requires at least one key/value pair"));
      }
    }

    let end = labels.last().unwrap().span.end;
    let span = Span::new(start, end);
    Ok((input, Instruction::Label(LabelInstruction { span, labels })))
  }

  /// Parses the undocumented but supported space-separated single label form
  /// (`LABEL name value`). The span includes the leading argument whitespace.
  fn label_single(&self, after_kw: &'a str) -> Option<(&'a str, Label)> {
    let input = self.arg_ws(after_kw)?;
    let (after_name, name) = self.label_name(input)?;
    // a single label is only valid when separated by whitespace (no `=`)
    let after_ws = self.arg_ws(after_name)?;
    let (rest, value) = self.label_value(after_ws)?;
    let span = Span::new(self.off(after_kw), value.span.end);
    Some((rest, Label { span, name, value }))
  }

  fn label_pair(&self, input: &'a str) -> Option<(&'a str, Label)> {
    let (after_name, name) = self.label_name(input)?;
    let after_eq = after_name.strip_prefix('=')?;
    let (rest, value) = self.label_value(after_eq)?;
    let span = Span::new(name.span.start, value.span.end);
    Some((rest, Label { span, name, value }))
  }

  /// A label name is either a quoted string or `any_equals` (text up to a
  /// space, newline or `=`).
  fn label_name(&self, input: &'a str) -> Option<(&'a str, SpannedString)> {
    if starts_with_quote(input) {
      return self.parse_quoted_string(input).ok();
    }
    let any_equals = take_while(|c: char| !is_ws(c) && !is_newline_char(c) && c != '=');
    let (rest, text) = if_not_empty(any_equals)(input).ok()?;
    Some((rest, self.spanned(input, rest, text.to_string())))
  }

  fn label_value(&self, input: &'a str) -> Option<(&'a str, SpannedString)> {
    if starts_with_quote(input) {
      return self.parse_quoted_string(input).ok();
    }
    let (rest, text) = self.any_whitespace(input).ok()?;
    Some((rest, self.spanned(input, rest, text.to_string())))
  }

  fn parse_env(&self, after_kw: &'a str, start: usize) -> PResult<'a, Instruction> {
    if let Some((rest, var)) = self.env_single(after_kw) {
      let span = Span::new(start, var.span.end);
      return Ok((rest, Instruction::Env(EnvInstruction { span, vars: vec![var] })));
    }

    // env_pairs = (arg_ws ~ env_pair?)+
    let mut vars = Vec::new();
    let mut input = after_kw;
    while let Some(after_ws) = self.arg_ws(input) {
      match self.env_pair(after_ws) {
        Some((rest, var)) => {
          vars.push(var);
          input = rest;
        }
        None => {
          input = after_ws;
          break;
        }
      }
    }
    if vars.is_empty() {
      return Err(fail("env requires a key/value pair"));
    }
    let span = Span::new(start, vars.last().unwrap().span.end);
    Ok((input, Instruction::Env(EnvInstruction { span, vars })))
  }

  /// The space-separated single env form (`ENV name value`), where the value
  /// may be a breakable multi-line string.
  fn env_single(&self, after_kw: &'a str) -> Option<(&'a str, EnvVar)> {
    let input = self.arg_ws(after_kw)?;
    let (after_name, key) = self.env_name(input)?;
    // a single env is only valid when separated by whitespace (no `=`)
    let after_ws = self.arg_ws(after_name)?;
    let (rest, value) = if starts_with_quote(after_ws) {
      let (rest, s) = self.parse_quoted_string(after_ws).ok()?;
      (rest, breakable_from_string(s))
    } else {
      self.any_breakable(after_ws).ok()?
    };
    let span = Span::new(key.span.start, value.span.end);
    Some((rest, EnvVar { span, key, value }))
  }

  fn env_pair(&self, input: &'a str) -> Option<(&'a str, EnvVar)> {
    let (after_name, key) = self.env_name(input)?;
    let after_eq = after_name.strip_prefix('=')?;
    let (rest, value) = if starts_with_quote(after_eq) {
      let (rest, s) = self.parse_quoted_string(after_eq).ok()?;
      (rest, breakable_from_string(s))
    } else {
      let (rest, value) = self.env_value(after_eq)?;
      (rest, breakable_from_string(value))
    };
    let span = Span::new(key.span.start, value.span.end);
    Some((rest, EnvVar { span, key, value }))
  }

  /// An unquoted env value. Like `any_whitespace` but a backslash escapes the
  /// following character (e.g. `\ ` keeps a space in the value), per Docker's
  /// rules. The span covers the raw text; the content has escapes resolved so
  /// the formatter can re-quote values that contain spaces.
  fn env_value(&self, input: &'a str) -> Option<(&'a str, SpannedString)> {
    let mut end = input.len();
    let mut chars = input.char_indices();
    while let Some((i, c)) = chars.next() {
      if c == '\\' {
        if line_continuation(&input[i..]).is_some() {
          end = i;
          break;
        }
        chars.next(); // the escaped character is part of the value
        continue;
      }
      if is_ws(c) || is_newline_char(c) {
        end = i;
        break;
      }
    }
    if end == 0 {
      return None;
    }
    let rest = &input[end..];
    let raw = &input[..end];
    Some((
      rest,
      SpannedString {
        span: self.span(input, rest),
        content: unescape(raw),
      },
    ))
  }

  fn env_name(&self, input: &'a str) -> Option<(&'a str, SpannedString)> {
    let name = take_while(|c: char| c.is_ascii_alphanumeric() || c == '_');
    let (rest, text) = if_not_empty(name)(input).ok()?;
    Some((rest, self.spanned(input, rest, text.to_string())))
  }

  fn parse_onbuild(&self, input: &'a str, start: usize) -> PResult<'a, Instruction> {
    let (rest, inner) = self.parse_instruction(input)?;
    let span = Span::new(start, inner.span().end);
    Ok((
      rest,
      Instruction::Onbuild(OnbuildInstruction {
        span,
        instruction: Box::new(inner),
      }),
    ))
  }

  fn parse_healthcheck(&self, input: &'a str, start: usize) -> PResult<'a, Instruction> {
    let mut flags = Vec::new();
    let mut input = input;
    // [OPTIONS] — the first arg_ws was consumed before the first token
    while let Some((rest, name, value, span)) = self.parse_flag_with(input, |c| c.is_ascii_alphanumeric() || c == '-') {
      flags.push(HealthcheckFlag { span, name, value });
      match self.arg_ws(rest) {
        Some(next) => input = next,
        None => {
          input = rest;
          break;
        }
      }
    }

    // the `NONE` form has no nested instruction
    if let Some(after) = strip_prefix_ci(input, "none")
      && (after.is_empty() || after.starts_with(is_ws) || starts_with_newline(after)) {
        let span = Span::new(start, self.off(after));
        return Ok((after, Instruction::Healthcheck(HealthcheckInstruction { span, flags, cmd: None })));
      }

    // otherwise a nested instruction follows (normally `CMD ...`)
    let (rest, inner) = self.parse_instruction(input)?;
    let span = Span::new(start, inner.span().end);
    Ok((
      rest,
      Instruction::Healthcheck(HealthcheckInstruction {
        span,
        flags,
        cmd: Some(Box::new(inner)),
      }),
    ))
  }

  fn parse_misc(&self, input: &'a str) -> PResult<'a, Instruction> {
    let start = self.off(input);
    let (after_kw, keyword) = alpha0(input);
    if keyword.is_empty() {
      return Err(fail("unexpected character"));
    }
    let instruction = self.spanned(input, after_kw, keyword.to_string());
    let (rest, arguments) = self.any_breakable(after_kw)?;
    let span = Span::new(start, arguments.span.end);
    Ok((rest, Instruction::Misc(MiscInstruction { span, instruction, arguments })))
  }

  // -- shared token parsers --

  /// Parses a `--name=value` flag whose name is ASCII-alphabetic (as in `FROM`
  /// and `COPY`). Returns `None` (a recoverable backtrace) if the input doesn't
  /// form a flag, so callers can treat it as another token.
  fn parse_flag(&self, input: &'a str) -> Option<(&'a str, SpannedString, SpannedString, Span)> {
    self.parse_flag_with(input, |c| c.is_ascii_alphabetic())
  }

  /// Like [`Parser::parse_flag`] but with a caller-supplied name character set
  /// (e.g. `HEALTHCHECK` flags such as `--start-period` allow dashes).
  fn parse_flag_with(&self, input: &'a str, name_char: impl Fn(char) -> bool) -> Option<(&'a str, SpannedString, SpannedString, Span)> {
    let after_dashes = input.strip_prefix("--")?;
    let (after_name, name_text) = if_not_empty(take_while(name_char))(after_dashes).ok()?;
    let after_eq = after_name.strip_prefix('=')?;
    let (rest, value_text) = self.any_whitespace(after_eq).ok()?;
    let name = self.spanned(after_dashes, after_name, name_text.to_string());
    let value = self.spanned(after_eq, rest, value_text.to_string());
    let span = Span::new(self.off(input), self.off(rest));
    Some((rest, name, value, span))
  }

  /// Parses a quoted string, returning a span that includes the surrounding
  /// quotes and content with the quotes removed and escapes resolved.
  fn parse_quoted_string(&self, input: &'a str) -> PResult<'a, SpannedString> {
    let quote = match input.chars().next() {
      Some(c @ ('"' | '\'' | '`')) => c,
      _ => return Err(fail("expected quoted string")),
    };
    let mut chars = input.char_indices();
    chars.next(); // opening quote
    let mut end = None;
    while let Some((i, c)) = chars.next() {
      if c == '\\' {
        chars.next(); // skip the escaped character
        continue;
      }
      if c == quote {
        end = Some(i + c.len_utf8());
        break;
      }
    }
    let Some(end) = end else {
      return Err(fail("unterminated quoted string"));
    };
    let rest = &input[end..];
    let content = unquote(&input[..end]);
    Ok((rest, self.spanned(input, rest, content)))
  }

  /// Parses a string array (`[ "a", "b" ]`) with the relaxed whitespace and
  /// optional trailing comma the grammar allows.
  fn string_array(&self, input: &'a str) -> PResult<'a, StringArray> {
    let start = input.strip_prefix('[').ok_or_else(|| fail("expected ["))?;
    let mut s = self.arg_ws_maybe(start);
    let mut elements = Vec::new();

    if let Some(rest) = s.strip_prefix(']') {
      return Ok((
        rest,
        StringArray {
          span: Span::new(self.off(input), self.off(rest)),
          elements,
        },
      ));
    }

    let (rest, first) = self.parse_quoted_string(s)?;
    elements.push(first);
    s = rest;
    loop {
      let after_ws = self.arg_ws_maybe(s);
      let Some(after_comma) = after_ws.strip_prefix(',') else {
        s = after_ws;
        break;
      };
      let after_comma = self.arg_ws_maybe(after_comma);
      if after_comma.starts_with(']') {
        s = after_comma;
        break;
      }
      let (rest, element) = self.parse_quoted_string(after_comma)?;
      elements.push(element);
      s = rest;
    }

    let s = self.arg_ws_maybe(s);
    let rest = s.strip_prefix(']').ok_or_else(|| fail("expected ]"))?;
    Ok((
      rest,
      StringArray {
        span: Span::new(self.off(input), self.off(rest)),
        elements,
      },
    ))
  }

  /// Parses a breakable string: content split across lines by `\` line
  /// continuations and interspersed with comment lines.
  fn any_breakable(&self, input: &'a str) -> PResult<'a, BreakableString> {
    let mut components: Vec<BreakableStringComponent> = Vec::new();
    let mut s = input;
    loop {
      let after_ws = skip_ws(s);
      if after_ws.starts_with('#') {
        // comment_line — leading whitespace is stripped (matching the grammar),
        // the formatter realigns the comment later
        let line_end = match after_ws.find(['\n', '\r']) {
          Some(i) => &after_ws[i..],
          None => "",
        };
        let content = &after_ws[..after_ws.len() - line_end.len()];
        let span = self.span(after_ws, line_end);
        components.push(BreakableStringComponent::Comment(SpannedComment {
          span,
          content: content.to_string(),
        }));
        // comment_line ~ NEWLINE? — and a comment always continues the string
        s = strip_newline(line_end).unwrap_or(line_end);
        if s.is_empty() {
          break;
        }
        continue;
      }

      let (after_content, content) = take_any_content(s);
      if content.is_empty() {
        break;
      }
      let span = self.span(s, after_content);
      components.push(BreakableStringComponent::String(SpannedString {
        span,
        content: content.to_string(),
      }));
      s = after_content;
      match line_continuation(s) {
        Some(rest) => {
          s = rest;
          if s.is_empty() {
            break;
          }
        }
        None => break,
      }
    }

    if components.is_empty() {
      return Err(fail("expected content"));
    }
    let end = component_end(components.last().unwrap());
    let span = Span::new(self.off(input), end);
    Ok((s, BreakableString { span, components }))
  }

  /// Consumes one or more units of argument whitespace: insignificant spaces or
  /// line continuations followed by any number of comment or empty lines.
  /// Returns `None` if nothing was consumed.
  fn arg_ws(&self, input: &'a str) -> Option<&'a str> {
    self.arg_ws_inner(input, true)
  }

  /// Like [`Parser::arg_ws`] but stops at a comment line rather than consuming
  /// it. Used before breakable shell content so a comment that follows the
  /// keyword's continuation (e.g. `RUN \` then `# note`) is preserved in the
  /// breakable string instead of being discarded.
  fn arg_ws_keep_comments(&self, input: &'a str) -> Option<&'a str> {
    self.arg_ws_inner(input, false)
  }

  fn arg_ws_inner(&self, input: &'a str, consume_comments: bool) -> Option<&'a str> {
    let mut s = input;
    loop {
      let after_ws = skip_ws(s);
      if after_ws.len() != s.len() {
        s = after_ws;
        continue;
      }
      let Some(after_cont) = line_continuation(s) else { break };
      s = after_cont;
      loop {
        if consume_comments
          && let Some(rest) = comment_line(s) {
            s = rest;
            continue;
          }
        if let Some(rest) = empty_line(s) {
          s = rest;
        } else {
          break;
        }
      }
    }
    if s.len() == input.len() { None } else { Some(s) }
  }

  fn arg_ws_maybe(&self, input: &'a str) -> &'a str {
    self.arg_ws(input).unwrap_or(input)
  }

  /// `any_whitespace`: consumes characters until insignificant whitespace, a
  /// newline, or a line continuation. Requires at least one character.
  fn any_whitespace(&self, input: &'a str) -> PResult<'a, &'a str> {
    let mut end = input.len();
    for (i, c) in input.char_indices() {
      if is_ws(c) || is_newline_char(c) || (c == '\\' && line_continuation(&input[i..]).is_some()) {
        end = i;
        break;
      }
    }
    if end == 0 {
      return Err(fail("expected argument"));
    }
    Ok((&input[end..], &input[..end]))
  }

  fn value_quoted_or(&self, input: &'a str, fallback: impl Fn(&Self, &'a str) -> PResult<'a, &'a str>) -> PResult<'a, SpannedString> {
    if starts_with_quote(input) {
      return self.parse_quoted_string(input);
    }
    let (rest, text) = fallback(self, input)?;
    Ok((rest, self.spanned(input, rest, text.to_string())))
  }

  /// After an instruction, consumes trailing whitespace and a single line
  /// separator. Tolerates a trailing comment on the same line.
  fn finish_line(&self, input: &'a str) -> Result<&'a str, ParseErrorFailureError> {
    let rest = skip_ws(input);
    if rest.is_empty() {
      return Ok(rest);
    }
    if let Some(rest) = strip_newline(rest) {
      return Ok(rest);
    }
    if rest.starts_with('#') {
      // a trailing comment line — already consumed by some breakable forms, but
      // tolerate it here too so the formatter can re-discover it in the gap
      return Ok(skip_to_next_line(rest));
    }
    Err(ParseErrorFailureError::new(format!("unexpected character at: {}", snippet(rest))))
  }

  /// If the just-parsed instruction's first line declares heredocs, consume
  /// their bodies (verbatim) and wrap the instruction in a [`HeredocInstruction`].
  /// `rest` is positioned at the newline ending the instruction's first line.
  fn maybe_consume_heredocs(&self, instruction: Instruction, rest: &'a str) -> (&'a str, Instruction) {
    let first_line = &self.base[instruction.span().start..self.off(rest)];
    let delimiters = find_heredoc_delimiters(first_line);
    if delimiters.is_empty() {
      return (rest, instruction);
    }
    // a body can only follow if there's a newline after the first line
    let Some(body_start) = strip_newline(rest) else {
      return (rest, instruction);
    };
    // bail (leaving the instruction unwrapped) if a heredoc is unterminated, so
    // a false-positive `<<` never swallows the rest of the file
    let Some((after, body_end)) = self.consume_heredoc_bodies(body_start, &delimiters) else {
      return (rest, instruction);
    };
    let body = self.base[self.off(body_start)..body_end].to_string();
    let span = Span::new(instruction.span().start, body_end);
    let instruction = Instruction::Heredoc(HeredocInstruction {
      span,
      instruction: Box::new(instruction),
      body,
    });
    (after, instruction)
  }

  /// Consumes heredoc bodies for `delimiters` in order, returning the input
  /// positioned at the newline after the final closing delimiter together with
  /// the byte offset where the body ends. Returns `None` if any heredoc reaches
  /// end-of-input without its closing delimiter.
  fn consume_heredoc_bodies(&self, body_start: &'a str, delimiters: &[Heredoc]) -> Option<(&'a str, usize)> {
    let mut cur = body_start;
    let mut final_rest = body_start;
    let mut end_off = self.off(body_start);
    for delim in delimiters {
      loop {
        if cur.is_empty() {
          return None;
        }
        let (line, after) = match cur.find(['\n', '\r']) {
          Some(i) => (&cur[..i], &cur[i..]),
          None => (cur, &cur[cur.len()..]),
        };
        let closed = delim.matches(line);
        end_off = self.off(after);
        final_rest = after;
        cur = strip_newline(after).unwrap_or(after);
        if closed {
          break;
        }
        if after.is_empty() {
          return None;
        }
      }
    }
    Some((final_rest, end_off))
  }

  // -- span helpers --

  /// The byte offset of `s` within the original input. `s` is normally a
  /// subslice of `base`, but combinators (including monch's) return a bare `""`
  /// literal when they consume to the end of input; that literal's pointer is
  /// not within `base`, so treat any out-of-range slice as end-of-input.
  fn off(&self, s: &'a str) -> usize {
    let base_start = self.base.as_ptr() as usize;
    let s_start = s.as_ptr() as usize;
    if s_start < base_start || s_start > base_start + self.base.len() {
      self.base.len()
    } else {
      s_start - base_start
    }
  }

  fn span(&self, from: &'a str, to: &'a str) -> Span {
    Span::new(self.off(from), self.off(to))
  }

  fn spanned(&self, from: &'a str, to: &'a str, content: String) -> SpannedString {
    SpannedString {
      span: self.span(from, to),
      content,
    }
  }
}

/// Which `ShellOrExecExpr`-carrying instruction is being parsed.
#[derive(Clone, Copy)]
enum ExprKind {
  Run,
  Cmd,
  Entrypoint,
  Shell,
}

impl ExprKind {
  fn build(self, span: Span, expr: ShellOrExecExpr) -> Instruction {
    match self {
      ExprKind::Run => Instruction::Run(RunInstruction { span, expr }),
      ExprKind::Cmd => Instruction::Cmd(CmdInstruction { span, expr }),
      ExprKind::Entrypoint => Instruction::Entrypoint(EntrypointInstruction { span, expr }),
      ExprKind::Shell => Instruction::Shell(ShellInstruction { span, expr }),
    }
  }
}

/// A heredoc declaration found on an instruction's first line.
struct Heredoc {
  /// The delimiter word (without surrounding quotes).
  word: String,
  /// `true` for the `<<-` form, where the closing delimiter may be tab-indented.
  strip_tabs: bool,
}

impl Heredoc {
  /// Whether `line` (already stripped of its line ending) closes this heredoc.
  fn matches(&self, line: &str) -> bool {
    if self.strip_tabs {
      line.trim_start_matches('\t') == self.word
    } else {
      line == self.word
    }
  }
}

/// Finds the heredoc declarations on an instruction's first line. Each
/// whitespace-separated token is checked, mirroring BuildKit's behavior: an
/// optional file descriptor, `<<` (or `<<-`), an optional quote, and a
/// delimiter that starts with a letter or underscore (so shell bit-shifts like
/// `$((1<<2))` are not mistaken for heredocs).
fn find_heredoc_delimiters(first_line: &str) -> Vec<Heredoc> {
  first_line.split_whitespace().filter_map(parse_heredoc_token).collect()
}

fn parse_heredoc_token(token: &str) -> Option<Heredoc> {
  let rest = token.trim_start_matches(|c: char| c.is_ascii_digit());
  let rest = rest.strip_prefix("<<")?;
  let (strip_tabs, rest) = match rest.strip_prefix('-') {
    Some(rest) => (true, rest),
    None => (false, rest),
  };
  let (quote, rest) = match rest.chars().next() {
    Some(q @ ('\'' | '"')) => (Some(q), &rest[1..]),
    _ => (None, rest),
  };

  // delimiter: [A-Za-z_][A-Za-z0-9_]*
  let mut chars = rest.char_indices();
  match chars.next() {
    Some((_, c)) if c.is_ascii_alphabetic() || c == '_' => {}
    _ => return None,
  }
  let mut end = rest.len();
  for (i, c) in chars {
    if !(c.is_ascii_alphanumeric() || c == '_') {
      end = i;
      break;
    }
  }
  let word = &rest[..end];
  let trailing = &rest[end..];

  // the token must be exactly the heredoc operator (plus a balanced closing
  // quote), otherwise it isn't a clean heredoc declaration
  match quote {
    Some(q) => {
      if trailing.len() != q.len_utf8() || !trailing.starts_with(q) {
        return None;
      }
    }
    None => {
      if !trailing.is_empty() {
        return None;
      }
    }
  }

  Some(Heredoc {
    word: word.to_string(),
    strip_tabs,
  })
}

fn breakable_from_string(s: SpannedString) -> BreakableString {
  BreakableString {
    span: s.span,
    components: vec![BreakableStringComponent::String(s)],
  }
}

fn component_end(component: &BreakableStringComponent) -> usize {
  match component {
    BreakableStringComponent::String(s) => s.span.end,
    BreakableStringComponent::Comment(c) => c.span.end,
  }
}

// -- free-function string scanners --

fn is_ws(c: char) -> bool {
  c == ' ' || c == '\t'
}

fn is_newline_char(c: char) -> bool {
  c == '\n' || c == '\r'
}

fn starts_with_newline(s: &str) -> bool {
  matches!(s.as_bytes().first(), Some(b'\n' | b'\r'))
}

fn starts_with_quote(s: &str) -> bool {
  matches!(s.chars().next(), Some('"' | '\'' | '`'))
}

fn skip_ws(s: &str) -> &str {
  skip_while(is_ws)(s).map(|(rest, _)| rest).unwrap_or(s)
}

/// Consumes a run of ASCII alphabetic characters (possibly empty).
fn alpha0(input: &str) -> (&str, &str) {
  take_while(|c: char| c.is_ascii_alphabetic())(input).unwrap_or((input, ""))
}

fn strip_newline(s: &str) -> Option<&str> {
  s.strip_prefix("\r\n").or_else(|| s.strip_prefix('\n')).or_else(|| s.strip_prefix('\r'))
}

fn skip_to_next_line(s: &str) -> &str {
  match s.find(['\n', '\r']) {
    Some(i) => strip_newline(&s[i..]).unwrap_or(&s[i..]),
    None => "",
  }
}

/// Strips a case-insensitive keyword prefix, returning the remaining input.
fn strip_prefix_ci<'a>(s: &'a str, keyword: &str) -> Option<&'a str> {
  if s.len() < keyword.len() || !s[..keyword.len()].eq_ignore_ascii_case(keyword) {
    return None;
  }
  Some(&s[keyword.len()..])
}

/// A line continuation: `\` followed by insignificant whitespace and a newline.
fn line_continuation(s: &str) -> Option<&str> {
  let rest = s.strip_prefix('\\')?;
  let rest = skip_ws(rest);
  strip_newline(rest)
}

/// `comment_line` = `ws* ~ comment ~ NEWLINE?`. Returns the remaining input.
fn comment_line(s: &str) -> Option<&str> {
  let rest = skip_ws(s);
  if !rest.starts_with('#') {
    return None;
  }
  Some(skip_to_next_line(rest))
}

/// `empty_line` = `ws* ~ NEWLINE`.
fn empty_line(s: &str) -> Option<&str> {
  strip_newline(skip_ws(s))
}

/// `any_content`: consumes characters until a newline or line continuation.
fn take_any_content(s: &str) -> (&str, &str) {
  for (i, c) in s.char_indices() {
    if is_newline_char(c) || (c == '\\' && line_continuation(&s[i..]).is_some()) {
      return (&s[i..], &s[..i]);
    }
  }
  ("", s)
}

/// Resolves backslash escapes in an unquoted value (e.g. `Rex\ The\ Dog` ->
/// `Rex The Dog`): a backslash drops out and the following character is kept
/// verbatim.
fn unescape(s: &str) -> String {
  let mut result = String::with_capacity(s.len());
  let mut chars = s.chars();
  while let Some(c) = chars.next() {
    if c == '\\' {
      match chars.next() {
        Some(next) => result.push(next),
        None => result.push('\\'),
      }
    } else {
      result.push(c);
    }
  }
  result
}

/// Removes the surrounding quotes from a quoted string and resolves escape
/// sequences, mirroring the `enquote` crate closely enough for Dockerfiles. The
/// caller guarantees `s` begins and ends with an ASCII quote character.
fn unquote(s: &str) -> String {
  if s.len() < 2 {
    return String::new();
  }
  let inner = &s[1..s.len() - 1]; // quotes are ASCII, so this is char-safe

  let mut result = String::with_capacity(inner.len());
  let mut iter = inner.chars();
  while let Some(c) = iter.next() {
    if c != '\\' {
      result.push(c);
      continue;
    }
    match iter.next() {
      Some('n') => result.push('\n'),
      Some('t') => result.push('\t'),
      Some('r') => result.push('\r'),
      Some('b') => result.push('\u{0008}'),
      Some('f') => result.push('\u{000C}'),
      Some('\\') => result.push('\\'),
      Some('"') => result.push('"'),
      Some('\'') => result.push('\''),
      Some('`') => result.push('`'),
      Some('\n') => {} // escaped newline collapses
      Some('u') => push_unicode_escape(&mut result, &mut iter, 4),
      Some('U') => push_unicode_escape(&mut result, &mut iter, 8),
      Some(other) => {
        result.push('\\');
        result.push(other);
      }
      None => result.push('\\'),
    }
  }
  result
}

/// Reads `digits` hex characters and pushes the corresponding `char`. On any
/// malformed sequence the bytes are emitted literally (matching `enquote`'s
/// lenient fallback rather than failing the parse).
fn push_unicode_escape(result: &mut String, iter: &mut std::str::Chars, digits: usize) {
  let marker = if digits == 4 { 'u' } else { 'U' };
  let mut hex = String::with_capacity(digits);
  for _ in 0..digits {
    match iter.next() {
      Some(c) if c.is_ascii_hexdigit() => hex.push(c),
      other => {
        // not a valid escape — emit what we consumed verbatim
        result.push('\\');
        result.push(marker);
        result.push_str(&hex);
        if let Some(c) = other {
          result.push(c);
        }
        return;
      }
    }
  }
  match u32::from_str_radix(&hex, 16).ok().and_then(char::from_u32) {
    Some(c) => result.push(c),
    None => {
      result.push('\\');
      result.push(marker);
      result.push_str(&hex);
    }
  }
}

fn fail(message: &'static str) -> ParseErrorFailureError {
  ParseErrorFailureError::new(message)
}

fn snippet(s: &str) -> String {
  match s.char_indices().nth(40) {
    Some((i, _)) => s[..i].to_string(),
    None => s.to_string(),
  }
}

#[cfg(test)]
mod test {
  use super::*;

  /// Spans are recovered from `&str` pointers, so inputs that consume to the end
  /// of the buffer (no trailing newline) must not panic.
  #[test]
  fn parses_without_trailing_newline() {
    let cases = [
      "FROM alpine",
      "FROM alpine:3.10 AS build",
      "FROM --platform=linux/amd64 node",
      "RUN echo hi",
      "RUN echo foo \\\n  # comment",
      "RUN \\\n  #only a comment",
      "CMD [\"a\", \"b\"]",
      "CMD [\"a\"",
      "EXPOSE 80",
      "ARG VERSION",
      "ARG VERSION=latest",
      "ENV A=B",
      "ENV A B",
      "ENV A=b\\ c",
      "LABEL a=b",
      "COPY a b",
      "SHELL [\"a\", \"b\"]",
      "SHELL not-an-array",
      "ONBUILD RUN echo hi",
      "ONBUILD",
      "HEALTHCHECK --interval=30s CMD curl",
      "HEALTHCHECK NONE",
      "HEALTHCHECK",
      "RUN <<EOF\nbody\nEOF",
      "RUN <<EOF\nbody",
      "COPY <<EOF /dest\nbody\nEOF",
      "",
      "   ",
      "# just a comment",
    ];
    for case in cases {
      let result = Dockerfile::parse(case);
      // must not panic; parse may legitimately error for some, but spans of any
      // produced instructions must stay within the input
      if let Ok(file) = result {
        for instruction in &file.instructions {
          let span = instruction.span();
          assert!(span.end <= case.len(), "span out of range for {case:?}: {span:?}");
          assert!(span.start <= span.end, "inverted span for {case:?}: {span:?}");
        }
      }
    }
  }

  #[test]
  fn unquotes_unicode_escapes() {
    assert_eq!(unquote(r#""café""#), "café");
    assert_eq!(unquote(r#""\U0001F600""#), "😀");
    assert_eq!(unquote(r#""a\tb\nc""#), "a\tb\nc");
    // malformed escapes are emitted verbatim rather than failing
    assert_eq!(unquote(r#""\u12""#), r"\u12");
    assert_eq!(unquote(r#""plain""#), "plain");
  }

  #[test]
  fn detects_heredoc_delimiters() {
    let one = |s| {
      let d = find_heredoc_delimiters(s);
      assert_eq!(d.len(), 1, "{s:?}");
      (d[0].word.clone(), d[0].strip_tabs)
    };
    assert_eq!(one("RUN <<EOF"), ("EOF".to_string(), false));
    assert_eq!(one("RUN <<-EOF"), ("EOF".to_string(), true));
    assert_eq!(one("RUN <<'EOF'"), ("EOF".to_string(), false));
    assert_eq!(one("RUN <<\"EOF\""), ("EOF".to_string(), false));
    assert_eq!(one("RUN python3 <<END"), ("END".to_string(), false));
    assert_eq!(one("RUN cat 2<<EOF"), ("EOF".to_string(), false));
    assert_eq!(find_heredoc_delimiters("RUN <<A <<B").len(), 2);
    // not heredocs
    assert!(find_heredoc_delimiters("RUN echo $((1<<2))").is_empty());
    assert!(find_heredoc_delimiters("RUN cat <<<EOF").is_empty());
    assert!(find_heredoc_delimiters("RUN echo a >> b").is_empty());
    assert!(find_heredoc_delimiters("RUN echo hi").is_empty());
  }

  #[test]
  fn parses_heredoc_body_verbatim() {
    let file = Dockerfile::parse("RUN <<EOF\n  indented\n\nblank above\nEOF\n").unwrap();
    match &file.instructions[0] {
      Instruction::Heredoc(h) => {
        assert_eq!(h.body, "  indented\n\nblank above\nEOF");
        assert!(matches!(&*h.instruction, Instruction::Run(_)));
      }
      other => panic!("expected heredoc, got {other:?}"),
    }
  }

  #[test]
  fn tab_indented_closing_delimiter_for_dash_form() {
    let file = Dockerfile::parse("RUN <<-EOF\n\tline\n\tEOF\n").unwrap();
    assert!(matches!(&file.instructions[0], Instruction::Heredoc(_)));
  }

  #[test]
  fn keeps_comment_after_run_continuation() {
    // a comment right after `RUN \` must not be discarded (#12): it becomes the
    // leading component of the breakable shell string
    let file = Dockerfile::parse("RUN \\\n# note\necho hi\n").unwrap();
    match &file.instructions[0] {
      Instruction::Run(run) => match &run.expr {
        ShellOrExecExpr::Shell(b) => {
          assert!(matches!(b.components.first(), Some(BreakableStringComponent::Comment(c)) if c.content == "# note"));
        }
        other => panic!("expected shell, got {other:?}"),
      },
      other => panic!("expected run, got {other:?}"),
    }
  }

  #[test]
  fn unterminated_heredoc_is_left_unwrapped() {
    // a `<<` whose delimiter never closes must not swallow the file or panic;
    // the instruction is left unwrapped (and the body lines parse on their own)
    let file = Dockerfile::parse("RUN <<EOF\nno closing delimiter\n").unwrap();
    assert!(!matches!(&file.instructions[0], Instruction::Heredoc(_)));
  }
}
