#![forbid(unsafe_code)]

use rigos_core::InspectionResult;
use rigos_machine::MachineContext;

pub trait MinerBackend {
    type Snapshot;
    fn discover(&self, machine: &MachineContext) -> InspectionResult<Self::Snapshot>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InspectedProcessIdentity {
    pid: u32,
}

impl InspectedProcessIdentity {
    pub fn new(pid: u32) -> Option<Self> {
        (pid > 0).then_some(Self { pid })
    }
    pub fn pid(self) -> u32 {
        self.pid
    }
}
