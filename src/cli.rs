use std::{
    ffi::{OsStr, OsString},
    path::PathBuf,
};

use anyhow::{bail, Result};

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum StartupRequest {
    Run(StartupOptions),
    Help,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct StartupOptions {
    pub pdf_path: Option<PathBuf>,
    pub smoke_open_pdf_path: Option<PathBuf>,
    pub gui_smoke: Option<GuiSmokeOptions>,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct GuiSmokeOptions {
    pub pdf_path: PathBuf,
    pub report_path: Option<PathBuf>,
}

impl StartupOptions {
    fn empty() -> Self {
        Self {
            pdf_path: None,
            smoke_open_pdf_path: None,
            gui_smoke: None,
        }
    }
}

pub fn parse_startup_options<I, S>(args: I) -> Result<StartupRequest>
where
    I: IntoIterator<Item = S>,
    S: Into<OsString>,
{
    let mut options = StartupOptions::empty();
    let mut args = args.into_iter().map(Into::into);

    while let Some(arg) = args.next() {
        if arg == OsStr::new("--help") || arg == OsStr::new("-h") {
            return Ok(StartupRequest::Help);
        }

        if arg == OsStr::new("--pdf") {
            if options.smoke_open_pdf_path.is_some() || options.gui_smoke.is_some() {
                bail!("--pdf cannot be combined with smoke modes");
            }
            if options.pdf_path.is_some() {
                bail!("--pdf can only be provided once");
            }

            options.pdf_path = Some(required_path_arg(&mut args, "--pdf")?);
            continue;
        }

        if arg == OsStr::new("--smoke-open-pdf") {
            if options.pdf_path.is_some() || options.gui_smoke.is_some() {
                bail!("--smoke-open-pdf cannot be combined with other PDF startup modes");
            }
            if options.smoke_open_pdf_path.is_some() {
                bail!("--smoke-open-pdf can only be provided once");
            }

            options.smoke_open_pdf_path = Some(required_path_arg(&mut args, "--smoke-open-pdf")?);
            continue;
        }

        if arg == OsStr::new("--gui-smoke") {
            if options.pdf_path.is_some() || options.smoke_open_pdf_path.is_some() {
                bail!("--gui-smoke cannot be combined with other PDF startup modes");
            }
            if options.gui_smoke.is_some() {
                bail!("--gui-smoke can only be provided once");
            }

            options.gui_smoke = Some(GuiSmokeOptions {
                pdf_path: required_path_arg(&mut args, "--gui-smoke")?,
                report_path: None,
            });
            continue;
        }

        if arg == OsStr::new("--gui-smoke-report") {
            let report_path = required_path_arg(&mut args, "--gui-smoke-report")?;
            let Some(gui_smoke) = options.gui_smoke.as_mut() else {
                bail!("--gui-smoke-report requires --gui-smoke");
            };
            if gui_smoke.report_path.is_some() {
                bail!("--gui-smoke-report can only be provided once");
            }

            gui_smoke.report_path = Some(report_path);
            continue;
        }

        bail!("unknown command-line option: {}", arg.to_string_lossy());
    }

    Ok(StartupRequest::Run(options))
}

pub fn help_text(program_name: &str) -> String {
    format!(
        "\
Quick Presenter

Usage: {program_name} [OPTIONS]

Options:
  --pdf <PATH>              Open a PDF at startup
  --smoke-open-pdf <PATH>   Open and render the first page, then exit
  --gui-smoke <PATH>        Open a PDF in Slint windows and run GUI smoke checks
  --gui-smoke-report <PATH> Write the GUI smoke report to a file
  -h, --help                Print help
"
    )
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
            StartupRequest::Run(StartupOptions {
                pdf_path: None,
                smoke_open_pdf_path: None,
                gui_smoke: None,
            })
        );
    }

    #[test]
    fn pdf_option_sets_startup_pdf_path() {
        let options = parse_startup_options(["--pdf", "deck.pdf"]).unwrap();

        assert_eq!(
            options,
            StartupRequest::Run(StartupOptions {
                pdf_path: Some(PathBuf::from("deck.pdf")),
                smoke_open_pdf_path: None,
                gui_smoke: None,
            })
        );
    }

    #[test]
    fn smoke_open_pdf_option_sets_smoke_pdf_path() {
        let options = parse_startup_options(["--smoke-open-pdf", "deck.pdf"]).unwrap();

        assert_eq!(
            options,
            StartupRequest::Run(StartupOptions {
                pdf_path: None,
                smoke_open_pdf_path: Some(PathBuf::from("deck.pdf")),
                gui_smoke: None,
            })
        );
    }

    #[test]
    fn gui_smoke_option_sets_gui_smoke_pdf_path() {
        let options = parse_startup_options(["--gui-smoke", "deck.pdf"]).unwrap();

        assert_eq!(
            options,
            StartupRequest::Run(StartupOptions {
                pdf_path: None,
                smoke_open_pdf_path: None,
                gui_smoke: Some(GuiSmokeOptions {
                    pdf_path: PathBuf::from("deck.pdf"),
                    report_path: None,
                }),
            })
        );
    }

    #[test]
    fn gui_smoke_report_sets_report_path() {
        let options =
            parse_startup_options(["--gui-smoke", "deck.pdf", "--gui-smoke-report", "out.txt"])
                .unwrap();

        assert_eq!(
            options,
            StartupRequest::Run(StartupOptions {
                pdf_path: None,
                smoke_open_pdf_path: None,
                gui_smoke: Some(GuiSmokeOptions {
                    pdf_path: PathBuf::from("deck.pdf"),
                    report_path: Some(PathBuf::from("out.txt")),
                }),
            })
        );
    }

    #[test]
    fn long_help_option_requests_help() {
        let request = parse_startup_options(["--help"]).unwrap();

        assert_eq!(request, StartupRequest::Help);
    }

    #[test]
    fn short_help_option_requests_help() {
        let request = parse_startup_options(["-h"]).unwrap();

        assert_eq!(request, StartupRequest::Help);
    }

    #[test]
    fn help_option_takes_precedence_over_other_arguments() {
        let request = parse_startup_options(["--help", "--pdf", "deck.pdf"]).unwrap();

        assert_eq!(request, StartupRequest::Help);
    }

    #[test]
    fn help_text_lists_supported_startup_arguments() {
        let help = help_text("qp");

        assert!(help.contains("Quick Presenter"));
        assert!(help.contains("Usage: qp [OPTIONS]"));
        assert!(help.contains("--pdf <PATH>"));
        assert!(help.contains("--smoke-open-pdf <PATH>"));
        assert!(help.contains("--gui-smoke <PATH>"));
        assert!(help.contains("--gui-smoke-report <PATH>"));
        assert!(help.contains("-h, --help"));
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
    fn gui_smoke_option_requires_value() {
        let err = parse_startup_options(["--gui-smoke"]).unwrap_err();

        assert!(err.to_string().contains("requires a PDF path"));
    }

    #[test]
    fn gui_smoke_report_requires_gui_smoke() {
        let err = parse_startup_options(["--gui-smoke-report", "out.txt"]).unwrap_err();

        assert!(err.to_string().contains("requires --gui-smoke"));
    }

    #[test]
    fn gui_smoke_report_rejects_empty_value() {
        let err = parse_startup_options(["--gui-smoke", "deck.pdf", "--gui-smoke-report", ""])
            .unwrap_err();

        assert!(err.to_string().contains("non-empty"));
    }

    #[test]
    fn duplicate_gui_smoke_option_is_rejected() {
        let err =
            parse_startup_options(["--gui-smoke", "a.pdf", "--gui-smoke", "b.pdf"]).unwrap_err();

        assert!(err.to_string().contains("only be provided once"));
    }

    #[test]
    fn unknown_option_is_rejected() {
        let err = parse_startup_options(["--fullscreen"]).unwrap_err();

        assert!(err.to_string().contains("unknown command-line option"));
    }
}
