use crate::{ContractError, MAX_FRAME_BYTES};
use serde::{de::DeserializeOwned, Serialize};

pub fn validate_declared_length(prefix: [u8; 4]) -> Result<usize, ContractError> {
    let length = u32::from_le_bytes(prefix) as usize;
    if !(1..=MAX_FRAME_BYTES).contains(&length) {
        return Err(ContractError::InvalidFrameLength(length));
    }
    Ok(length)
}

pub fn encode_frame<T: Serialize>(value: &T) -> Result<Vec<u8>, ContractError> {
    let payload = serde_json::to_vec(value)?;
    if !(1..=MAX_FRAME_BYTES).contains(&payload.len()) {
        return Err(ContractError::InvalidFrameLength(payload.len()));
    }
    let mut frame = Vec::with_capacity(payload.len() + 4);
    frame.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    frame.extend_from_slice(&payload);
    Ok(frame)
}

pub fn decode_frame<T: DeserializeOwned>(frame: &[u8]) -> Result<T, ContractError> {
    let prefix: [u8; 4] = frame
        .get(..4)
        .ok_or(ContractError::TruncatedFrame)?
        .try_into()
        .map_err(|_| ContractError::TruncatedFrame)?;
    let declared = validate_declared_length(prefix)?;
    if frame.len() < declared + 4 {
        return Err(ContractError::TruncatedFrame);
    }
    if frame.len() > declared + 4 {
        return Err(ContractError::TrailingBytes);
    }
    Ok(serde_json::from_slice(&frame[4..])?)
}
