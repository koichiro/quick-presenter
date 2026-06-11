use std::{
    ffi::{OsStr, OsString},
    path::PathBuf,
};

use anyhow::{bail, Result};

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct StartupOptions {
    pub pdf_path: Option<PathBuf>,
    pub smoke_open_pdf_path: Option<PathBuf>,
}

impl StartupOptions {
    fn empty() -> Self {
        Self {
            pdf_path: None,
            smoke_open_pdf_path: None,
        }
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
            if options.smoke_open_pdf_path.is_some() {
                bail!("--pdf cannot be combined with --smoke-open-pdf");
            }
            if options.pdf_path.is_some() {
                bail!("--pdf can only be provided once");
            }

            options.pdf_path = Some(required_path_arg(&mut args, "--pdf")?);
            continue;
        }

        if arg == OsStr::new("--smoke-open-pdf") {
            if options.pdf_path.is_some() {
                bail!("--smoke-open-pdf cannot be combined with --pdf");
            }
            if options.smoke_open_pdf_path.is_some() {
                bail!("--smoke-open-pdf can only be provided once");
            }

            options.smoke_open_pdf_path = Some(required_path_arg(&mut args, "--smoke-open-pdf")?);
            continue;
        }

        bail!("unknown command-line option: {}", arg.to_string_lossy());
    }

    Ok(options)
}

fn required_path_arg<I>(args: &mut I, option: &str) -> Result<PathBuf>
where
    I: Iterator<Item = OsString>,
{
    let Some(path) = args.next() else {
        bail!("{option} requires a PDF path");
    };

    if path.as_os_str().is_empty() {
        bail!("{option} requires a non-empty PDF path");
    }

    Ok(PathBuf::from(path))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_arguments_produces_empty_options() {
        let options = parse_startup_options(Vec::<&str>::new()).unwrap();

        assert_eq!(
            options,
            StartupOptions {
                pdf_path: None,
                smoke_open_pdf_path: None,
            }
        );
    }

    #[test]
    fn pdf_option_sets_startup_pdf_path() {
        let options = parse_startup_options(["--pdf", "deck.pdf"]).unwrap();

        assert_eq!(
            options,
            StartupOptions {
                pdf_path: Some(PathBuf::from("deck.pdf")),
                smoke_open_pdf_path: None,
            }
        );
    }

    #[test]
    fn smoke_open_pdf_option_sets_smoke_pdf_path() {
        let options = parse_startup_options(["--smoke-open-pdf", "deck.pdf"]).unwrap();

        assert_eq!(
            options,
            StartupOptions {
                pdf_path: None,
                smoke_open_pdf_path: Some(PathBuf::from("deck.pdf")),
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
    fn smoke_open_pdf_option_requires_value() {
        let err = parse_startup_options(["--smoke-open-pdf"]).unwrap_err();

        assert!(err.to_string().contains("requires a PDF path"));
    }

    #[test]
    fn smoke_open_pdf_option_rejects_empty_value() {
        let err = parse_startup_options(["--smoke-open-pdf", ""]).unwrap_err();

        assert!(err.to_string().contains("non-empty"));
    }

    #[test]
    fn duplicate_smoke_open_pdf_option_is_rejected() {
        let err = parse_startup_options(["--smoke-open-pdf", "a.pdf", "--smoke-open-pdf", "b.pdf"])
            .unwrap_err();

        assert!(err.to_string().contains("only be provided once"));
    }

    #[test]
    fn pdf_and_smoke_open_pdf_options_cannot_be_combined() {
        let err =
            parse_startup_options(["--pdf", "a.pdf", "--smoke-open-pdf", "b.pdf"]).unwrap_err();

        assert!(err.to_string().contains("cannot be combined"));
    }

    #[test]
    fn unknown_option_is_rejected() {
        let err = parse_startup_options(["--fullscreen"]).unwrap_err();

        assert!(err.to_string().contains("unknown command-line option"));
    }
}
