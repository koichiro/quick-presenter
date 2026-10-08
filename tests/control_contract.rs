//! Historical wire/CLI examples protect protocol v1 field names, types, and semantics.
use quick_presenter::control::{client, protocol::*};
use serde_json::Value;
fn fixture() -> Value {
    serde_json::from_str(include_str!("fixtures/control-v1.json")).unwrap()
}
fn contains_contract(actual: &Value, expected: &Value) {
    match expected {
        Value::Object(fields) => {
            assert!(actual.is_object());
            for (key, value) in fields {
                assert!(actual.get(key).is_some(), "Missing required field: {key}");
                contains_contract(&actual[key], value);
            }
        }
        _ => assert_eq!(actual, expected),
    }
}
#[test]
fn v1_requests_preserve_legacy_compact_params_and_explicit_full_params() {
    for expected in fixture()["requests"].as_array().unwrap() {
        let mut expected = expected.clone();
        if cfg!(windows) && expected["method"] == "presentation.open" {
            expected["params"]["file"] = Value::String("C:\\slides\\demo.pdf".into());
        }
        let request = decode_request(&serde_json::to_vec(&expected).unwrap()).unwrap();
        assert_eq!(serde_json::to_value(request).unwrap(), expected);
    }
}
#[test]
fn v1_responses_and_cli_json_retain_required_fields() {
    for expected in fixture()["responses"].as_array().unwrap() {
        let response: Response = serde_json::from_value(expected.clone()).unwrap();
        contains_contract(&serde_json::to_value(&response).unwrap(), expected);
        assert_eq!(response.protocol_version, 1);
        assert_eq!(response.id, Some(42));
        let Outcome::Result(reply) = response.outcome else {
            panic!("fixture must be a success");
        };
        if matches!(
            reply,
            Reply::Status(_)
                | Reply::TimerElapsed { .. }
                | Reply::Notes { .. }
                | Reply::Slide { .. }
                | Reply::Context { .. }
                | Reply::Mutation { .. }
        ) {
            let output = client::format_reply(&reply, true).unwrap();
            assert_eq!(output.lines().count(), 1);
            let actual: Value = serde_json::from_str(&output).unwrap();
            assert_eq!(actual["protocol_version"], 1);
            assert!(actual.get("id").is_none());
            contains_contract(&actual, &expected["result"]);
        }
    }
}
#[test]
fn v1_ndjson_events_retain_the_document_and_sequence_baseline() {
    for expected in fixture()["events"].as_array().unwrap() {
        let event: EventEnvelope = serde_json::from_value(expected.clone()).unwrap();
        let output = client::format_event(&event, true).unwrap();
        assert_eq!(output.lines().count(), 1);
        contains_contract(&serde_json::from_str(&output).unwrap(), expected);
    }
}
#[test]
fn v1_machine_error_codes_and_exit_categories_remain_stable() {
    for (name, exit) in fixture()["error_exits"].as_object().unwrap() {
        let code: ErrorCode = serde_json::from_value(Value::String(name.clone())).unwrap();
        assert_eq!(code.exit_code() as i64, exit.as_i64().unwrap());
        let response = Response::error(Some(42), code, "Diagnostic text is not an API key.");
        let actual = serde_json::to_value(response).unwrap();
        assert_eq!(actual["protocol_version"], 1);
        assert_eq!(actual["id"], 42);
        assert_eq!(actual["error"]["code"], *name);
        assert!(actual.get("result").is_none());
    }
}
#[test]
fn version_output_is_available_without_creating_an_ipc_endpoint() {
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_qp"))
        .args(["--version", "--json"])
        .env("XDG_RUNTIME_DIR", "/a/nonexistent/runtime/directory")
        .env("PDFIUM_DYNAMIC_LIB_PATH", "/a/nonexistent/pdfium/library")
        .output()
        .unwrap();
    assert!(output.status.success());
    assert!(output.stderr.is_empty());
    let actual: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        actual,
        serde_json::json!({"application_version":env!("CARGO_PKG_VERSION"),"protocol_version":1})
    );
}
