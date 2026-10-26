//! SPM IPC host capability and bounded transport.
mod host;
pub use host::{PipeIpc, PipeState};
use pecofence_plugin_api::{ScopeId, Token};
use spm_contracts::{ProjectQuery, Request};
use std::sync::{
    Arc,
    atomic::{AtomicU8, Ordering},
};
use tokio::sync::mpsc;

#[cfg(any(windows, test))]
mod actor;
#[cfg(windows)]
pub use actor::{probe_current, run};

pub const MAX_OPERATIONS: usize = 64;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum ConnectionState {
    Connecting = 0,
    Connected = 1,
    Reconnecting = 2,
    Disconnected = 3,
}

#[derive(Clone)]
pub struct TransportHandle {
    data_sender: mpsc::Sender<Command>,
    control_sender: mpsc::UnboundedSender<Command>,
    state: Arc<AtomicU8>,
}

impl TransportHandle {
    pub fn subscribe(
        &self,
        local: Token,
        owner: ScopeId,
        generation: u64,
        query: ProjectQuery,
    ) -> Result<(), DispatchError> {
        self.data_sender
            .try_send(Command::Subscribe {
                local,
                route: Route { owner, generation },
                query,
            })
            .map_err(send_error)
    }

    pub fn unsubscribe(&self, local: Token) -> Result<(), DispatchError> {
        self.control_sender
            .send(Command::Unsubscribe { local })
            .map_err(|_| DispatchError::Closed)
    }

    pub fn request(
        &self,
        local: Token,
        owner: ScopeId,
        generation: u64,
        request: Request,
    ) -> Result<(), DispatchError> {
        self.data_sender
            .try_send(Command::Request {
                local,
                route: Route { owner, generation },
                request,
            })
            .map_err(send_error)
    }

    pub fn cancel_operation(&self, local: Token) -> Result<(), DispatchError> {
        self.control_sender
            .send(Command::CancelOperation { local })
            .map_err(|_| DispatchError::Closed)
    }

    #[allow(dead_code)]
    pub fn connection_state(&self) -> ConnectionState {
        match self.state.load(Ordering::Acquire) {
            0 => ConnectionState::Connecting,
            1 => ConnectionState::Connected,
            2 => ConnectionState::Reconnecting,
            _ => ConnectionState::Disconnected,
        }
    }
}

/// Dispatch was rejected before an operation reached the actor.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DispatchError {
    Full,
    Closed,
}
impl std::fmt::Display for DispatchError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "SPM transport dispatch: {self:?}")
    }
}
impl std::error::Error for DispatchError {}
fn send_error(error: mpsc::error::TrySendError<Command>) -> DispatchError {
    match error {
        mpsc::error::TrySendError::Full(_) => {
            tracing::warn!("transport.data_commands_full");
            DispatchError::Full
        }
        mpsc::error::TrySendError::Closed(_) => DispatchError::Closed,
    }
}

#[cfg_attr(not(any(windows, test)), allow(dead_code))]
pub struct Receivers {
    pub(crate) data: mpsc::Receiver<Command>,
    pub(crate) control: mpsc::UnboundedReceiver<Command>,
}

// PipeState::token uses a checked, increasing u64 counter; tokens never repeat.
// An unsubscribe may overtake its subscribe across these independent channels.
pub fn channel() -> (TransportHandle, Receivers) {
    let (data_sender, data) = mpsc::channel(64);
    let (control_sender, control) = mpsc::unbounded_channel();
    let state = Arc::new(AtomicU8::new(ConnectionState::Disconnected as u8));
    (
        TransportHandle {
            data_sender,
            control_sender,
            state,
        },
        Receivers { data, control },
    )
}

#[derive(Clone, Copy)]
#[cfg_attr(not(any(windows, test)), allow(dead_code))]
pub(crate) struct Route {
    owner: ScopeId,
    generation: u64,
}

// ProjectQuery/Envelope payloads dominate the variant size; the transport
// clones these commands across the actor boundary and boxing would churn
// every construction site for no measurable benefit.
#[allow(clippy::large_enum_variant)]
#[cfg_attr(not(any(windows, test)), allow(dead_code))]
pub(crate) enum Command {
    Subscribe {
        local: Token,
        route: Route,
        query: ProjectQuery,
    },
    Unsubscribe {
        local: Token,
    },
    Request {
        local: Token,
        route: Route,
        request: Request,
    },
    CancelOperation {
        local: Token,
    },
}
