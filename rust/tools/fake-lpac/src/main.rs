use std::{
    env,
    io::{self, Read},
    process,
};

use serde_json::{Value, json};

const ACTIVATION_CODE: &str = "LPA:1$rsp.test$MATCH-123";
const CONFIRMATION_CODE: &str = "1234";
const ICCID: &str = "8901000000000000001";

fn emit(event_type: &str, code: i32, message: &str, data: Value) {
    println!(
        "{}",
        json!({
            "type": event_type,
            "payload": {
                "code": code,
                "message": message,
                "data": data,
            }
        })
    );
}

fn fail(message: &str, data: Value) -> ! {
    emit("lpa", -1, message, data);
    process::exit(1);
}

fn main() {
    if env::var("LPAC_APDU_PCSC_DRV_IFID").as_deref() == Ok("999") {
        fail("pcsc_reader_unavailable", json!("fake reader failure"));
    }

    let args = env::args().skip(1).collect::<Vec<_>>();
    let command = args.iter().map(String::as_str).collect::<Vec<_>>();

    match command.as_slice() {
        ["chip", "info"] => emit(
            "lpa",
            0,
            "success",
            json!({
                "eidValue": "89049032000000000000000000000001"
            }),
        ),
        ["profile", "list"] => emit(
            "lpa",
            0,
            "success",
            json!([{
                "iccid": ICCID,
                "profileState": "disabled",
                "serviceProviderName": "NIK Test",
                "profileName": "Fake integration profile"
            }]),
        ),
        ["profile", "download", "-a", "-", "-c", "-"] => {
            if args.iter().any(|argument| {
                argument.contains("rsp.test") || argument.contains(CONFIRMATION_CODE)
            }) {
                fail("secret_present_in_argv", Value::Null);
            }

            let mut input = String::new();
            if io::stdin().read_to_string(&mut input).is_err() {
                fail("stdin_read_failed", Value::Null);
            }
            let mut lines = input.lines();
            if lines.next() != Some(ACTIVATION_CODE)
                || lines.next() != Some(CONFIRMATION_CODE)
                || lines.next().is_some()
            {
                fail("unexpected_stdin_payload", Value::Null);
            }

            emit(
                "progress",
                0,
                "es9p_initiate_authentication",
                json!("rsp.test"),
            );
            emit(
                "progress",
                0,
                "es10b_load_bound_profile_package",
                Value::Null,
            );
            emit(
                "lpa",
                0,
                "success",
                json!({
                    "iccid": ICCID,
                    "stdinValidated": true,
                    "argvSecretFree": true
                }),
            );
        }
        _ => fail("unsupported_fake_command", json!(command)),
    }
}
