use std::error::Error;
use std::fmt::{Debug, Display, Formatter, Result, Write};

/// Wrapper to truncate output from debug formatting.
pub struct TruncateDebug<'a, T: Debug> {
    value: &'a T,
    max_len: usize,
}

impl<'a, T: Debug> TruncateDebug<'a, T> {
    /// Wrap a value with default maximum debug output length.
    pub fn new(value: &'a T) -> Self {
        Self::with_max_len(value, 128)
    }

    /// Wrap a value with a specified maximum debug output length.
    pub fn with_max_len(value: &'a T, max_len: usize) -> Self {
        Self { value, max_len }
    }
}

impl<'a, T: Debug> Debug for TruncateDebug<'a, T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        let mut remaining = self.max_len;
        let mut writer = TruncateWriter {
            formatter: f,
            remaining: &mut remaining,
        };

        let _ = write!(&mut writer, "{:?}", self.value);

        if remaining == 0 {
            write!(f, "...")?;
        }

        Ok(())
    }
}

struct TruncateWriter<'a, 'b> {
    formatter: &'a mut Formatter<'b>,
    remaining: &'a mut usize,
}

impl<'a, 'b> Write for TruncateWriter<'a, 'b> {
    fn write_str(&mut self, s: &str) -> Result {
        if *self.remaining == 0 {
            return Ok(());
        }

        let write_len = s.len().min(*self.remaining);
        self.formatter.write_str(&s[..write_len])?;
        *self.remaining -= write_len;

        Ok(())
    }
}

/// Wrapper to write error chain for Display formatting.
pub struct ErrorChainDisplay<'a, E: Error>(pub &'a E);

impl<'a, E: Error> Display for ErrorChainDisplay<'a, E> {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        write!(f, "{}", self.0)?;
        let mut source = self.0.source();
        while let Some(cause) = source {
            write!(f, ": {}", cause)?;
            source = cause.source();
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_truncate_debug() {
        #[allow(dead_code)]
        #[derive(Debug)]
        enum Foo {
            Data(Vec<u8>),
            Text(String),
        }

        let foo = Foo::Data(vec![0, 1, 2, 3, 4, 5, 6, 7, 8, 9]);
        assert_eq!(
            format!("{:?}", TruncateDebug::with_max_len(&foo, 20)),
            "Data([0, 1, 2, 3, 4,..."
        );

        let foo = Foo::Text("To be or not to be, that is the question.".to_owned());
        assert_eq!(
            format!("{:?}", TruncateDebug::with_max_len(&foo, 20)),
            r#"Text("To be or not t..."#
        );
    }

    #[test]
    fn test_error_alt_display() {
        #[derive(Debug, thiserror::Error)]
        enum E {
            #[error("v")]
            V,
        }

        #[derive(Debug, thiserror::Error)]
        enum E2 {
            V2(#[source] E),
        }

        impl Display for E2 {
            fn fmt(&self, f: &mut Formatter<'_>) -> Result {
                write!(f, "v2")
            }
        }

        #[derive(Debug, thiserror::Error)]
        enum E3 {
            #[error("v3")]
            V3(#[source] E2),
        }

        let err = E3::V3(E2::V2(E::V));
        let alt = ErrorChainDisplay(&err);
        assert_eq!(format!("{err}"), "v3");
        assert_eq!(format!("{alt}"), "v3: v2: v");
    }
}
