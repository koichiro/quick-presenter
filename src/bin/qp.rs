use quick_presenter::control::{
    client::{self, CliRequest},
    protocol::{Command, ControlError, ErrorCode, Outcome, Request},
};
use std::io::{self, Write};
fn main() {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    let json = args.iter().any(|arg| arg == "--json");
    let result = run(args);
    if let Err(error) = result {
        if json {
            let _ = writeln!(
                io::stderr(),
                "{}",
                serde_json::to_string(&error).expect("error JSON")
            );
        } else {
            let _ = writeln!(io::stderr(), "qp: {:?}: {}", error.code, error.message);
        }
        std::process::exit(error.code.exit_code());
    }
}
fn run(args: Vec<std::ffi::OsString>) -> Result<(), ControlError> {
    let output = match client::parse_args(args)? {
        CliRequest::Help => client::HELP.into(),
        CliRequest::Version { json } => client::format_version(json)
            .map_err(|e| ControlError::new(ErrorCode::IpcFailure, e.to_string()))?,
        CliRequest::Run(options) => {
            let request = Request::new(1, options.command);
            if matches!(request.command, Command::Watch(_)) {
                let mut stdout = io::stdout().lock();
                return client::watch(&request, |event| {
                    let output = client::format_event(&event, options.json).map_err(|error| {
                        ControlError::new(ErrorCode::IpcFailure, error.to_string())
                    })?;
                    stdout
                        .write_all(output.as_bytes())
                        .and_then(|_| stdout.flush())
                        .map_err(|error| {
                            ControlError::new(ErrorCode::IpcFailure, error.to_string())
                        })
                });
            }
            let response = client::send(&request)?;
            match response.outcome {
                Outcome::Error(error) => return Err(error),
                Outcome::Result(reply) => client::format_reply(&reply, options.json)
                    .map_err(|e| ControlError::new(ErrorCode::IpcFailure, e.to_string()))?,
            }
        }
    };
    io::stdout()
        .write_all(output.as_bytes())
        .map_err(|e| ControlError::new(ErrorCode::IpcFailure, e.to_string()))
}
