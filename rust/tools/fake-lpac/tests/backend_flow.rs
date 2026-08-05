use lpac_backend::LegacyLpacBackend;
use lpac_core::ActivationCode;

const ACTIVATION_CODE: &str = "LPA:1$rsp.test$MATCH-123";
const CONFIRMATION_CODE: &str = "1234";
const ICCID: &str = "8901000000000000001";

fn backend() -> LegacyLpacBackend {
    LegacyLpacBackend::new(env!("CARGO_BIN_EXE_fake-lpac")).with_reader_index(Some(0))
}

#[test]
fn downloads_through_stdin_and_verifies_profile_list() {
    let code = ActivationCode::parse(ACTIVATION_CODE).unwrap();
    let run = backend()
        .download_and_verify(&code, Some(CONFIRMATION_CODE))
        .unwrap();

    assert_eq!(run.result.data["iccid"], ICCID);
    assert_eq!(run.result.data["stdinValidated"], true);
    assert_eq!(run.result.data["argvSecretFree"], true);
    assert!(
        run.progress
            .iter()
            .any(|event| event.message == "post_install_profile_list_verified")
    );

    let log = run.pretty_log();
    assert!(!log.contains(ACTIVATION_CODE));
    assert!(!log.contains(CONFIRMATION_CODE));
}

#[test]
fn propagates_structured_reader_failure() {
    let backend =
        LegacyLpacBackend::new(env!("CARGO_BIN_EXE_fake-lpac")).with_reader_index(Some(999));
    let error = backend.chip_info().unwrap_err().to_string();

    assert!(error.contains("pcsc_reader_unavailable"));
    assert!(error.contains("fake reader failure"));
}
