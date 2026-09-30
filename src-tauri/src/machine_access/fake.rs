use std::{
    collections::VecDeque,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

use crate::{domain::Machine, machine_access::MachineAccess};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum FakeMachineAccessCall {
    Shell {
        machine_id: i64,
        command: String,
    },
    FindExecutable {
        machine_id: i64,
        name: String,
    },
    HomeDirectory {
        machine_id: i64,
    },
    WriteFile {
        machine_id: i64,
        path: PathBuf,
        contents: Vec<u8>,
    },
    ProvisionPstackTree {
        machine_id: i64,
    },
}

#[derive(Debug, Clone, Default)]
pub(crate) struct FakeMachineAccess {
    calls: Arc<Mutex<Vec<FakeMachineAccessCall>>>,
    shell_results: Arc<Mutex<VecDeque<Result<String, String>>>>,
    pstack_failure: Arc<Mutex<Option<String>>>,
}

impl FakeMachineAccess {
    pub(crate) fn new(shell_results: impl IntoIterator<Item = Result<String, String>>) -> Self {
        Self {
            calls: Arc::default(),
            shell_results: Arc::new(Mutex::new(shell_results.into_iter().collect())),
            pstack_failure: Arc::new(Mutex::new(None)),
        }
    }

    pub(crate) fn calls(&self) -> Vec<FakeMachineAccessCall> {
        self.calls.lock().expect("fake calls lock").clone()
    }

    pub(crate) fn fail_pstack_provisioning(&self, error: impl Into<String>) {
        *self
            .pstack_failure
            .lock()
            .expect("fake pstack failure lock") = Some(error.into());
    }

    fn record(&self, call: FakeMachineAccessCall) {
        self.calls.lock().expect("fake calls lock").push(call);
    }
}

impl MachineAccess for FakeMachineAccess {
    fn run_shell(&self, machine: &Machine, command: &str) -> Result<String, String> {
        self.record(FakeMachineAccessCall::Shell {
            machine_id: machine.id,
            command: command.to_owned(),
        });
        self.shell_results
            .lock()
            .expect("fake shell results lock")
            .pop_front()
            .unwrap_or_else(|| Ok(String::new()))
    }

    fn find_executable(&self, machine: &Machine, name: &str) -> Result<PathBuf, String> {
        self.record(FakeMachineAccessCall::FindExecutable {
            machine_id: machine.id,
            name: name.to_owned(),
        });
        Ok(PathBuf::from(format!("/fake/bin/{name}")))
    }

    fn home_directory(&self, machine: &Machine) -> Result<PathBuf, String> {
        self.record(FakeMachineAccessCall::HomeDirectory {
            machine_id: machine.id,
        });
        Ok(PathBuf::from("/fake/home"))
    }

    fn home_path(&self, _machine: &Machine) -> String {
        "/fake/home".into()
    }

    fn is_local(&self, machine: &Machine) -> bool {
        matches!(machine.transport, crate::domain::MachineTransport::Local)
    }

    fn resolve_path(&self, _machine: &Machine, path: &str) -> PathBuf {
        PathBuf::from("/fake/home").join(path.trim_start_matches("~/"))
    }

    fn write_file(&self, machine: &Machine, path: &Path, contents: &[u8]) -> Result<(), String> {
        self.record(FakeMachineAccessCall::WriteFile {
            machine_id: machine.id,
            path: path.to_owned(),
            contents: contents.to_vec(),
        });
        Ok(())
    }

    fn provision_pstack_tree(&self, machine: &Machine) -> Result<String, String> {
        self.record(FakeMachineAccessCall::ProvisionPstackTree {
            machine_id: machine.id,
        });
        if let Some(error) = self
            .pstack_failure
            .lock()
            .expect("fake pstack failure lock")
            .clone()
        {
            return Err(error);
        }
        Ok("/fake/home/.local/share/ai-mission-manager/pstack/fake".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{MachineObservation, MachineTransport};

    #[test]
    fn fake_machine_access_records_shell_home_and_file_calls() {
        let access = FakeMachineAccess::default();
        let machine = Machine {
            id: 4,
            context_id: 2,
            name: "test".into(),
            socket_name: "test".into(),
            transport: MachineTransport::Local,
            last_observed: MachineObservation::Unknown,
            last_observed_at: None,
        };
        access.run_shell(&machine, "printf test").unwrap();
        assert_eq!(
            access.home_directory(&machine).unwrap(),
            PathBuf::from("/fake/home")
        );
        access
            .write_file(&machine, Path::new("/fake/home/role.md"), b"role")
            .unwrap();

        assert!(matches!(
            access.calls().as_slice(),
            [
                FakeMachineAccessCall::Shell { machine_id: 4, .. },
                FakeMachineAccessCall::HomeDirectory { machine_id: 4 },
                FakeMachineAccessCall::WriteFile { machine_id: 4, contents, .. }
            ] if contents == b"role"
        ));
    }
}
