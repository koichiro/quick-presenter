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
    pub log_file_path: Option<PathBuf>,
    pub config_path: Option<PathBuf>,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct GuiSmokeOptions {
    pub pdf_path: PathBuf,
    pub report_path: Option<PathBuf>,
}

impl StartupOptions {
    pub fn is_smoke(&self) -> bool {
        self.smoke_open_pdf_path.is_some() || self.gui_smoke.is_some()
    }

    fn empty() -> Self {
        Self {
            pdf_path: None,
            smoke_open_pdf_path: None,
            gui_smoke: None,
            log_file_path: None,
            config_path: None,
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

        if arg == OsStr::new("--config") {
            if options.config_path.is_some() {
                bail!("--config can only be provided once");
            }
            let path = required_path_arg(&mut args, "--config")?;
            if path.as_os_str().to_string_lossy().starts_with('-') {
                bail!("--config requires a path, not an option");
            }
            options.config_path = Some(path);
            continue;
        }

        if arg == OsStr::new("--log-file") {
            if options.log_file_path.is_some() {
                bail!("--log-file can only be provided once");
            }

            options.log_file_path = Some(required_path_arg(&mut args, "--log-file")?);
            continue;
        }

        if arg.to_string_lossy().starts_with('-') {
            bail!("unknown command-line option: {}", arg.to_string_lossy());
        }

        if options.pdf_path.is_some()
            || options.smoke_open_pdf_path.is_some()
            || options.gui_smoke.is_some()
        {
            bail!("PDF path cannot be combined with startup options");
        }

        options.pdf_path = Some(PathBuf::from(arg));
    }

    Ok(StartupRequest::Run(options))
}

pub fn help_text(program_name: &str) -> String {
    format!(
        "\
Quick Presenter

Usage: {program_name} [OPTIONS] [PDF_PATH]

Options:
  --pdf <PATH>              Open a PDF at startup
  --smoke-open-pdf <PATH>   Open and render the first page, then exit
  --gui-smoke <PATH>        Open a PDF in Slint windows and run GUI smoke checks
  --gui-smoke-report <PATH> Write the GUI smoke report to a file
  --config <PATH>           Use this TOML file instead of the standard user configuration
  --log-file <PATH>         Write diagnostic logs to a specific file
  -h, --help                Print help
"
    )
}

fn required_path_arg<I>(args: &mut I, option: &str) -> Result<PathBuf>
where
    I: Iterator<Item = OsString>,
{
    let Some(path) = args.next() else {
        bail!("{option} requires a path");
    };

    if path.as_os_str().is_empty() {
        bail!("{option} requires a non-empty path");
    }

    Ok(PathBuf::from(path))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_can_follow_pdf_and_coexist_with_each_smoke_mode() {
        for mode in ["--pdf", "--smoke-open-pdf", "--gui-smoke"] {
            for args in [
                vec![mode, "deck.pdf", "--config", "settings with spaces.toml"],
                vec!["--config", "settings with spaces.toml", mode, "deck.pdf"],
            ] {
                let StartupRequest::Run(options) = parse_startup_options(args).unwrap() else {
                    panic!("expected startup options");
                };
                assert_eq!(
                    options.config_path,
                    Some(PathBuf::from("settings with spaces.toml"))
                );
                assert_eq!(options.is_smoke(), mode != "--pdf");
                match mode {
                    "--pdf" => assert_eq!(options.pdf_path, Some(PathBuf::from("deck.pdf"))),
                    "--smoke-open-pdf" => {
                        assert_eq!(options.smoke_open_pdf_path, Some(PathBuf::from("deck.pdf")))
                    }
                    _ => assert_eq!(
                        options.gui_smoke.unwrap().pdf_path,
                        PathBuf::from("deck.pdf")
                    ),
                }
            }
        }
    }

    #[test]
    fn config_alone_does_not_select_a_pdf_or_smoke_mode() {
        let StartupRequest::Run(options) =
            parse_startup_options(["--config", "./-settings.toml"]).unwrap()
        else {
            panic!("expected startup options");
        };
        assert_eq!(options.config_path, Some(PathBuf::from("./-settings.toml")));
        assert!(options.pdf_path.is_none());
        assert!(!options.is_smoke());
        assert!(help_text("quick-presenter").contains("--config <PATH>"));
    }

    #[test]
    fn config_option_is_explicit_and_independent_of_pdf() {
        let StartupRequest::Run(options) =
            parse_startup_options(["--config", "debug.toml", "deck.pdf"]).unwrap()
        else {
            panic!()
        };
        assert_eq!(options.config_path, Some(PathBuf::from("debug.toml")));
        assert_eq!(options.pdf_path, Some(PathBuf::from("deck.pdf")));
        for args in [
            vec!["--config"],
            vec!["--config", ""],
            vec!["--config", "--pdf"],
            vec!["--config", "a", "--config", "b"],
        ] {
            assert!(parse_startup_options(args).is_err());
        }
    }

    #[cfg(unix)]
    #[test]
    fn config_preserves_non_utf8_paths() {
        use std::os::unix::ffi::OsStringExt;
        let path = OsString::from_vec(vec![b'a', 255]);
        let StartupRequest::Run(options) =
            parse_startup_options([OsString::from("--config"), path.clone()]).unwrap()
        else {
            panic!()
        };
        assert_eq!(options.config_path, Some(PathBuf::from(path)));
    }

    #[test]
    fn no_arguments_produces_empty_options() {
        let options = parse_startup_options(Vec::<&str>::new()).unwrap();

        assert_eq!(
            options,
            StartupRequest::Run(StartupOptions {
                pdf_path: None,
                smoke_open_pdf_path: None,
                gui_smoke: None,
                log_file_path: None,
                config_path: None,
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
                log_file_path: None,
                config_path: None,
            })
        );
    }

    #[test]
    fn positional_pdf_path_sets_startup_pdf_path() {
        let options = parse_startup_options(["deck.pdf"]).unwrap();

        assert_eq!(
            options,
            StartupRequest::Run(StartupOptions {
                pdf_path: Some(PathBuf::from("deck.pdf")),
                smoke_open_pdf_path: None,
                gui_smoke: None,
                log_file_path: None,
                config_path: None,
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
                log_file_path: None,
                config_path: None,
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
                log_file_path: None,
                config_path: None,
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
                log_file_path: None,
                config_path: None,
            })
        );
    }

    #[test]
    fn log_file_option_sets_diagnostic_log_path() {
        let options = parse_startup_options(["--log-file", "quick-presenter.log"]).unwrap();

        assert_eq!(
            options,
            StartupRequest::Run(StartupOptions {
                pdf_path: None,
                smoke_open_pdf_path: None,
                gui_smoke: None,
                log_file_path: Some(PathBuf::from("quick-presenter.log")),
                config_path: None,
            })
        );
    }

    #[test]
    fn log_file_option_can_be_combined_with_pdf_startup() {
        let options =
            parse_startup_options(["--log-file", "app.log", "--pdf", "deck.pdf"]).unwrap();

        assert_eq!(
            options,
            StartupRequest::Run(StartupOptions {
                pdf_path: Some(PathBuf::from("deck.pdf")),
                smoke_open_pdf_path: None,
                gui_smoke: None,
                log_file_path: Some(PathBuf::from("app.log")),
                config_path: None,
            })
        );
    }

    #[test]
    fn log_file_option_can_be_combined_with_smoke_modes() {
        let options =
            parse_startup_options(["--gui-smoke", "deck.pdf", "--log-file", "app.log"]).unwrap();

        assert_eq!(
            options,
            StartupRequest::Run(StartupOptions {
                pdf_path: None,
                smoke_open_pdf_path: None,
                gui_smoke: Some(GuiSmokeOptions {
                    pdf_path: PathBuf::from("deck.pdf"),
                    report_path: None,
                }),
                log_file_path: Some(PathBuf::from("app.log")),
                config_path: None,
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
        let help = help_text("quick-presenter");

        assert!(help.contains("Quick Presenter"));
        assert!(help.contains("Usage: quick-presenter [OPTIONS] [PDF_PATH]"));
        assert!(help.contains("--pdf <PATH>"));
        assert!(help.contains("--config <PATH>"));
        assert!(help.contains("--smoke-open-pdf <PATH>"));
        assert!(help.contains("--gui-smoke <PATH>"));
        assert!(help.contains("--gui-smoke-report <PATH>"));
        assert!(help.contains("--log-file <PATH>"));
        assert!(help.contains("-h, --help"));
    }

    #[test]
    fn pdf_option_requires_value() {
        let err = parse_startup_options(["--pdf"]).unwrap_err();

        assert!(err.to_string().contains("requires a path"));
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
    fn pdf_option_and_positional_pdf_path_cannot_be_combined() {
        let err = parse_startup_options(["--pdf", "a.pdf", "b.pdf"]).unwrap_err();

        assert!(err.to_string().contains("cannot be combined"));
    }

    #[test]
    fn smoke_open_pdf_option_and_positional_pdf_path_cannot_be_combined() {
        let err = parse_startup_options(["--smoke-open-pdf", "a.pdf", "b.pdf"]).unwrap_err();

        assert!(err.to_string().contains("cannot be combined"));
    }

    #[test]
    fn smoke_open_pdf_option_requires_value() {
        let err = parse_startup_options(["--smoke-open-pdf"]).unwrap_err();

        assert!(err.to_string().contains("requires a path"));
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

        assert!(err.to_string().contains("requires a path"));
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
    fn log_file_option_requires_value() {
        let err = parse_startup_options(["--log-file"]).unwrap_err();

        assert!(err.to_string().contains("requires a path"));
    }

    #[test]
    fn log_file_option_rejects_empty_value() {
        let err = parse_startup_options(["--log-file", ""]).unwrap_err();

        assert!(err.to_string().contains("non-empty"));
    }

    #[test]
    fn duplicate_log_file_option_is_rejected() {
        let err =
            parse_startup_options(["--log-file", "a.log", "--log-file", "b.log"]).unwrap_err();

        assert!(err.to_string().contains("only be provided once"));
    }

    #[test]
    fn unknown_option_is_rejected() {
        let err = parse_startup_options(["--fullscreen"]).unwrap_err();

        assert!(err.to_string().contains("unknown command-line option"));
    }
}
