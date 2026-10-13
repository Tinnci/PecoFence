use spm_contracts::*;
use std::collections::BTreeSet;
use uuid::Uuid;

#[test]
fn hello_request_matches_canonical_fixture() {
    let envelope = Envelope {
        protocol_major: PROTOCOL_MAJOR,
        protocol_minor: PROTOCOL_MINOR,
        daemon_session: None,
        request_id: Some(RequestId(
            Uuid::parse_str("aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa").unwrap(),
        )),
        subscription_id: None,
        revision: None,
        body: Body::Request(Request::Hello(HelloRequest {
            client_build: "pecofence-test".into(),
            protocol_major: 2,
            protocol_minor: 0,
            features: BTreeSet::from([Feature::Multiplexing, Feature::Pagination]),
        })),
    };
    envelope.validate(Direction::ClientToServer).unwrap();
    assert_eq!(
        canonical_json(&envelope).unwrap(),
        include_str!("../fixtures/rpc/hello-request.json")
    );
    let decoded: Envelope =
        serde_json::from_str(include_str!("../fixtures/rpc/hello-request.json")).unwrap();
    assert_eq!(decoded, envelope);
}
