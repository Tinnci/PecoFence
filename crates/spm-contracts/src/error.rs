use crate::RequestId;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ContractError {
    #[error("invalid {field}: {reason}")]
    InvalidField { field: &'static str, reason: String },
    #[error("invalid frame length {0}")]
    InvalidFrameLength(usize),
    #[error("frame is truncated")]
    TruncatedFrame,
    #[error("frame has trailing bytes")]
    TrailingBytes,
    #[error("invalid JSON: {0}")]
    Json(#[from] serde_json::Error),
    #[error("invalid envelope: {0}")]
    InvalidEnvelope(String),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    ProtocolMismatch,
    UnsupportedFeature,
    InvalidRequest,
    UnknownScope,
    UnknownSubscription,
    SnapshotExpired,
    Backpressure,
    Unavailable,
    PermissionDenied,
    Cancelled,
    DeadlineExceeded,
    NotFound,
    Conflict,
    Internal,
}

pub type ErrorDetails = BTreeMap<String, serde_json::Value>;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ErrorDto {
    pub code: ErrorCode,
    pub request_id: Option<RequestId>,
    pub retryable: bool,
    pub message: String,
    pub details: Option<ErrorDetails>,
}

impl ErrorDto {
    pub fn validate(&self) -> Result<(), ContractError> {
        let permitted = matches!(
            self.code,
            ErrorCode::Backpressure | ErrorCode::Unavailable | ErrorCode::DeadlineExceeded
        );
        if self.retryable && !permitted {
            return Err(ContractError::InvalidField {
                field: "retryable",
                reason: format!("not permitted for {:?}", self.code),
            });
        }
        if self.message.trim().is_empty() || self.message.len() > 1024 {
            return Err(ContractError::InvalidField {
                field: "message",
                reason: "must contain 1..=1024 bytes".into(),
            });
        }
        Ok(())
    }
}
