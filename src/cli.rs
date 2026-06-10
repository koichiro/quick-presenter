use std::{
    ffi::{OsStr, OsString},
    path::PathBuf,
};

use anyhow::{bail, Result};

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct StartupOptions {
    pub pdf_path: Option<PathBuf>,
}

impl StartupOptions {
    fn empty() -> Self {
        Self { pdf_path: None }
    }
}

pub fn parse_startup_options<I, S>(args: I) -> Result<StartupOptions>
where
    I: IntoIterator<Item = S>,
    S: Into<OsString>,
{
    let mut options = StartupOptions::empty();
    let mut args = args.into_iter().map(Into::into);

    while let Some(arg) = args.next() {
        if arg == OsStr::new("--pdf") {
            if options.pdf_path.is_some() {
                bail!("--pdf can only be provided once");
            }

            let Some(path) = args.next() else {
                bail!("--pdf requires a PDF path");
            };

            if path.as_os_str().is_empty() {
                bail!("--pdf requires a non-empty PDF path");
            }

            options.pdf_path = Some(PathBuf::from(path));
            continue;
        }

        bail!("unknown command-line option: {}", arg.to_string_lossy());
    }

    Ok(options)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_arguments_produces_empty_options() {
        let options = parse_startup_options(Vec::<&str>::new()).unwrap();

        assert_eq!(options, StartupOptions { pdf_path: None });
    }

    #[test]
    fn pdf_option_sets_startup_pdf_path() {
        let options = parse_startup_options(["--pdf", "deck.pdf"]).unwrap();

        assert_eq!(
            options,
            StartupOptions {
                pdf_path: Some(PathBuf::from("deck.pdf")),
            }
        );
    }

    #[test]
    fn pdf_option_requires_value() {
        let err = parse_startup_options(["--pdf"]).unwrap_err();

        assert!(err.to_string().contains("requires a PDF path"));
    }

    #[test]
    fn pdf_option_rejects_empty_value() {
        let err = parse_startup_options(["--pdf", ""]).unwrap_err();

        assert!(err.to_string().contains("non-empty"));
    }

    #[test]
    fn duplicate_pdf_option_is_rejected() {
        let err = parse_startup_options(["--pdf", "a.pdf", "--pdf", "b.pdf"]).unwrap_err();

        assert!(err.to_string().contains("only be provided once"));
    }

    #[test]
    fn unknown_option_is_rejected() {
        let err = parse_startup_options(["--fullscreen"]).unwrap_err();

        assert!(err.to_string().contains("unknown command-line option"));
    }
}
