use lpac_backend::{LegacyLpacBackend, LpacRun, PcscReader};
use lpac_core::ActivationCode;
use lpac_relay_backend::{RelayAgentProcess, RelayAgentRole};
use serde_json::Value;
use std::{
    path::PathBuf,
    sync::mpsc::{self, Receiver, Sender, TryRecvError},
    thread,
};
use uuid::Uuid;

pub enum WorkerCommand {
    DiscoverReaders {
        executable: PathBuf,
    },
    ChipInfo {
        executable: PathBuf,
        reader_index: u32,
    },
    Profiles {
        executable: PathBuf,
        reader_index: u32,
    },
    LocalDownload {
        executable: PathBuf,
        reader_index: u32,
        activation_code: String,
        confirmation_code: Option<String>,
    },
    StartRelay {
        executable: PathBuf,
        role: RelayAgentRole,
        reader_index: Option<u32>,
    },
    RelayRequest {
        token: Uuid,
        operation: String,
        payload: Value,
    },
    StopRelay,
    Shutdown,
}

pub enum WorkerEvent {
    Readers(Result<Vec<PcscReader>, String>),
    ChipInfo(Result<LpacRun, String>),
    Profiles(Result<LpacRun, String>),
    LocalDownload(Result<LpacRun, String>),
    RelayStarted(Result<RelayAgentRole, String>),
    RelayResponse {
        token: Uuid,
        operation: String,
        result: Result<Value, String>,
    },
    RelayStopped(Result<(), String>),
}

pub struct WorkerController {
    command_tx: Sender<WorkerCommand>,
    event_rx: Receiver<WorkerEvent>,
}

impl WorkerController {
    pub fn spawn() -> Self {
        let (command_tx, command_rx) = mpsc::channel();
        let (event_tx, event_rx) = mpsc::channel();
        thread::spawn(move || worker_loop(command_rx, event_tx));
        Self {
            command_tx,
            event_rx,
        }
    }

    pub fn send(&self, command: WorkerCommand) -> Result<(), String> {
        self.command_tx
            .send(command)
            .map_err(|_| "Фоновий процес NIK LPA завершився".to_owned())
    }

    pub fn try_recv(&self) -> Result<Option<WorkerEvent>, String> {
        match self.event_rx.try_recv() {
            Ok(event) => Ok(Some(event)),
            Err(TryRecvError::Empty) => Ok(None),
            Err(TryRecvError::Disconnected) => {
                Err("Канал фонового процесу NIK LPA закритий".to_owned())
            }
        }
    }
}

impl Drop for WorkerController {
    fn drop(&mut self) {
        let _ = self.command_tx.send(WorkerCommand::Shutdown);
    }
}

fn worker_loop(command_rx: Receiver<WorkerCommand>, event_tx: Sender<WorkerEvent>) {
    let mut relay: Option<RelayAgentProcess> = None;

    while let Ok(command) = command_rx.recv() {
        match command {
            WorkerCommand::DiscoverReaders { executable } => {
                let result = LegacyLpacBackend::new(executable)
                    .readers()
                    .map_err(|error| format!("{error:#}"));
                let _ = event_tx.send(WorkerEvent::Readers(result));
            }
            WorkerCommand::ChipInfo {
                executable,
                reader_index,
            } => {
                let result = LegacyLpacBackend::new(executable)
                    .with_reader_index(Some(reader_index))
                    .chip_info()
                    .map_err(|error| format!("{error:#}"));
                let _ = event_tx.send(WorkerEvent::ChipInfo(result));
            }
            WorkerCommand::Profiles {
                executable,
                reader_index,
            } => {
                let result = LegacyLpacBackend::new(executable)
                    .with_reader_index(Some(reader_index))
                    .profiles()
                    .map_err(|error| format!("{error:#}"));
                let _ = event_tx.send(WorkerEvent::Profiles(result));
            }
            WorkerCommand::LocalDownload {
                executable,
                reader_index,
                activation_code,
                confirmation_code,
            } => {
                let result = ActivationCode::parse(activation_code)
                    .map_err(|error| error.to_string())
                    .and_then(|code| {
                        LegacyLpacBackend::new(executable)
                            .with_reader_index(Some(reader_index))
                            .download_and_verify(&code, confirmation_code.as_deref())
                            .map_err(|error| format!("{error:#}"))
                    });
                let _ = event_tx.send(WorkerEvent::LocalDownload(result));
            }
            WorkerCommand::StartRelay {
                executable,
                role,
                reader_index,
            } => {
                if let Some(process) = relay.take() {
                    let _ = process.shutdown();
                }
                let result = RelayAgentProcess::start(executable, role, reader_index)
                    .map(|process| {
                        relay = Some(process);
                        role
                    })
                    .map_err(|error| format!("{error:#}"));
                let _ = event_tx.send(WorkerEvent::RelayStarted(result));
            }
            WorkerCommand::RelayRequest {
                token,
                operation,
                payload,
            } => {
                let result = relay
                    .as_mut()
                    .ok_or_else(|| "Relay Agent не запущений".to_owned())
                    .and_then(|process| {
                        process
                            .request(&operation, payload)
                            .map_err(|error| format!("{error:#}"))
                    });
                let _ = event_tx.send(WorkerEvent::RelayResponse {
                    token,
                    operation,
                    result,
                });
            }
            WorkerCommand::StopRelay => {
                let result = relay
                    .take()
                    .map(RelayAgentProcess::shutdown)
                    .transpose()
                    .map_err(|error| format!("{error:#}"));
                let _ = event_tx.send(WorkerEvent::RelayStopped(result));
            }
            WorkerCommand::Shutdown => {
                if let Some(process) = relay.take() {
                    let _ = process.shutdown();
                }
                break;
            }
        }
    }
}
