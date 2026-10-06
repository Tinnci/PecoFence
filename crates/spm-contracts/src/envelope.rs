use crate::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Direction {
    ClientToServer,
    ServerToClient,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "payload", rename_all = "snake_case")]
// `Error(ErrorDto)` is by far the largest variant, but this enum is serialized
// immediately on the wire and error paths are cold; boxing would touch every
// consumer's pattern matches for no measurable gain.
#[allow(clippy::large_enum_variant)]
pub enum Body {
    Request(Request),
    Response(Response),
    Event(Event),
    Error(ErrorDto),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Envelope {
    pub protocol_major: u16,
    pub protocol_minor: u16,
    pub daemon_session: Option<DaemonSessionId>,
    pub request_id: Option<RequestId>,
    pub subscription_id: Option<SubscriptionId>,
    pub revision: Option<Revision>,
    #[serde(flatten)]
    pub body: Body,
}

impl Envelope {
    pub fn validate(&self, direction: Direction) -> Result<(), ContractError> {
        if self.protocol_major != PROTOCOL_MAJOR {
            return Err(ContractError::InvalidEnvelope(
                "unsupported protocol major".into(),
            ));
        }
        match &self.body {
            Body::Request(Request::Hello(request)) => {
                if direction != Direction::ClientToServer
                    || self.daemon_session.is_some()
                    || self.request_id.is_none()
                    || self.subscription_id.is_some()
                    || self.revision.is_some()
                {
                    return Err(ContractError::InvalidEnvelope(
                        "invalid hello request fields".into(),
                    ));
                }
                if request.protocol_major != self.protocol_major {
                    return Err(ContractError::InvalidEnvelope(
                        "hello major differs from envelope".into(),
                    ));
                }
            }
            Body::Response(Response::Hello(response)) => {
                if direction != Direction::ServerToClient
                    || self.request_id.is_none()
                    || self.daemon_session != Some(response.daemon_session)
                    || self.subscription_id.is_some()
                    || self.revision.is_some()
                {
                    return Err(ContractError::InvalidEnvelope(
                        "invalid hello response fields".into(),
                    ));
                }
            }
            Body::Request(request) => {
                if direction != Direction::ClientToServer
                    || self.daemon_session.is_none()
                    || self.request_id.is_none()
                {
                    return Err(ContractError::InvalidEnvelope(
                        "business request requires session and request id".into(),
                    ));
                }
                request.validate()?;
            }
            Body::Response(_) => {
                if direction != Direction::ServerToClient
                    || self.daemon_session.is_none()
                    || self.request_id.is_none()
                {
                    return Err(ContractError::InvalidEnvelope(
                        "response requires session and request id".into(),
                    ));
                }
            }
            Body::Error(error) => {
                if direction != Direction::ServerToClient
                    || self.request_id.is_none()
                    || error.request_id != self.request_id
                {
                    return Err(ContractError::InvalidEnvelope(
                        "error request id mismatch".into(),
                    ));
                }
                error.validate()?;
            }
            Body::Event(Event::Snapshot(snapshot)) => {
                if direction != Direction::ServerToClient
                    || self.request_id.is_some()
                    || self.daemon_session.is_none()
                    || self.subscription_id.is_none()
                    || self.revision.is_none()
                {
                    return Err(ContractError::InvalidEnvelope(
                        "snapshot event fields are incomplete".into(),
                    ));
                }
                if self.daemon_session != Some(snapshot.daemon_session)
                    || self.revision != Some(snapshot.revision)
                {
                    return Err(ContractError::InvalidEnvelope(
                        "snapshot identity differs from envelope".into(),
                    ));
                }
                snapshot.validate()?;
            }
            Body::Event(Event::OperationCompleted(_)) => {
                if direction != Direction::ServerToClient
                    || self.daemon_session.is_none()
                    || self.request_id.is_some()
                    || self.subscription_id.is_some()
                    || self.revision.is_some()
                {
                    return Err(ContractError::InvalidEnvelope(
                        "invalid operation event fields".into(),
                    ));
                }
            }
        }
        Ok(())
    }
}
