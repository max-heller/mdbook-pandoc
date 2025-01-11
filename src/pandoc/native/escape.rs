use std::{
    fmt::{self, Display, Write as _},
    io, mem,
};

pub trait Escape {
    fn escape_quotes(&self) -> impl Display;
    fn escape_quotes_verbatim(&self) -> impl Display;
}

#[derive(Default)]
enum State {
    #[default]
    Init,
    Backslash,
}

impl State {
    fn step(&mut self, c: char) -> Option<(&str, char)> {
        match self {
            Self::Backslash => {
                let out = match c {
                    '\\' => Some((r"\", '\\')),
                    '"' => Some((r"\", '"')),
                    c => Some((r"\\", c)),
                };
                *self = Self::Init;
                out
            }
            Self::Init => match c {
                '\\' => {
                    *self = State::Backslash;
                    None
                }
                '"' => Some((r"\", '"')),
                c => Some(("", c)),
            },
        }
    }

    fn finish(self) -> &'static str {
        match self {
            Self::Init => "",
            Self::Backslash => r"\\",
        }
    }
}

impl Escape for str {
    fn escape_quotes(&self) -> impl Display {
        struct Escaped<'a>(&'a str);

        impl Display for Escaped<'_> {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                let mut state = State::Init;
                for c in self.0.chars() {
                    if let Some((prefix, c)) = state.step(c) {
                        f.write_str(prefix)?;
                        f.write_char(c)?;
                    }
                }
                f.write_str(state.finish())
            }
        }

        Escaped(self)
    }

    fn escape_quotes_verbatim(&self) -> impl Display {
        struct Escaped<'a>(&'a str);

        impl Display for Escaped<'_> {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                for c in self.0.chars() {
                    match c {
                        '"' => write!(f, r#"\""#)?,
                        '\\' => write!(f, r#"\\"#)?,
                        c => write!(f, "{c}")?,
                    }
                }
                Ok(())
            }
        }

        Escaped(self)
    }
}

pub struct Writer<W: io::Write> {
    utf8: utf8parse::Parser,
    receiver: Receiver<W>,
}

struct Receiver<W> {
    state: State,
    writer: W,
    err: Option<io::Error>,
}

impl<W: io::Write> Writer<W> {
    pub fn new(writer: W) -> Self {
        Self {
            utf8: utf8parse::Parser::new(),
            receiver: Receiver {
                state: Default::default(),
                writer,
                err: None,
            },
        }
    }

    pub fn unescaped(&mut self) -> &mut W {
        &mut self.receiver.writer
    }

    pub fn start_text(&mut self) -> io::Result<()> {
        self.unescaped().write_all(br#"""#)
    }

    pub fn end_text(&mut self) -> io::Result<()> {
        let state = mem::take(&mut self.receiver.state);
        match self.receiver.err.take() {
            None => {
                self.unescaped().write_all(state.finish().as_bytes())?;
                self.unescaped().write_all(br#"""#)
            }
            Some(err) => Err(err),
        }
    }
}

impl<W: io::Write> utf8parse::Receiver for Receiver<W> {
    fn codepoint(&mut self, c: char) {
        if let Some((prefix, c)) = self.state.step(c) {
            match write!(self.writer, "{prefix}{c}") {
                Ok(()) => {}
                Err(err) => self.err = Some(err),
            }
        }
    }

    fn invalid_sequence(&mut self) {}
}

impl<W: io::Write> io::Write for Writer<W> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        for byte in buf {
            self.utf8.advance(&mut self.receiver, *byte);
        }
        match self.receiver.err.take() {
            None => Ok(buf.len()),
            Some(err) => Err(err),
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        self.receiver.writer.flush()
    }
}

#[cfg(test)]
mod tests {
    use super::Escape;

    #[test]
    fn escape_quotes() {
        assert_eq!(r#"a"b"#.escape_quotes().to_string(), r#"a\"b"#,);
        assert_eq!(r#"a\"b"#.escape_quotes().to_string(), r#"a\"b"#);
        assert_eq!(r#"a\\"b"#.escape_quotes().to_string(), r#"a\\\"b"#);
        assert_eq!(r#"a\\\"b"#.escape_quotes().to_string(), r#"a\\\"b"#);
        assert_eq!(r#"\"#.escape_quotes().to_string(), r#"\\"#);
    }
}
