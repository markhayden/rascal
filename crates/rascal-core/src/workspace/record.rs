//! The operation record: the immutable, durable account of one committed invocation,
//! `ops/<identity>.json`. Its identity is the hash of everything but two observations,
//! `duration_ms` and the commit sequence number.

use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::canonical;
use crate::identity::Identity;
use crate::op::Envelope;

/// One committed invocation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct OperationRecord {
    /// The record's version.
    pub record_version: u32,
    /// The step id the invocation ran as.
    pub step: String,
    pub op: String,
    pub version: String,
    /// The resolved parameters.
    pub params: serde_json::Value,
    /// Input identities by port.
    pub inputs: BTreeMap<String, String>,
    /// Output identities by port.
    pub outputs: BTreeMap<String, String>,
    pub envelope: Envelope,
    /// The constant set, when the op used one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub constants: Option<String>,
    /// `sha256:` over everything above.
    pub identity: String,
    /// Wall time; an observation outside the identity.
    pub duration_ms: u64,
    /// The workspace sequence number this record committed at; outside the identity.
    pub seq: u64,
}

impl OperationRecord {
    /// A record with its identity computed; `seq` is set at commit.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        step: &str,
        op: &str,
        version: &str,
        params: serde_json::Value,
        inputs: BTreeMap<String, String>,
        outputs: BTreeMap<String, String>,
        envelope: Envelope,
        constants: Option<String>,
        duration_ms: u64,
    ) -> OperationRecord {
        let mut record = OperationRecord {
            record_version: 1,
            step: step.to_string(),
            op: op.to_string(),
            version: version.to_string(),
            params,
            inputs,
            outputs,
            envelope,
            constants,
            identity: String::new(),
            duration_ms,
            seq: 0,
        };
        record.identity = record.compute_identity().as_str().to_string();
        record
    }

    /// The identity over everything but the observations.
    pub fn compute_identity(&self) -> Identity {
        let mut envelope = self.envelope.clone();
        envelope.provenance.duration_ms = 0;
        canonical::identity_of(
            "operation",
            &serde_json::json!({
                "step": self.step, "op": self.op, "version": self.version, "params": self.params,
                "inputs": self.inputs, "outputs": self.outputs, "envelope": envelope,
                "constants": self.constants,
            }),
        )
    }

    /// The identity as a typed value.
    pub fn identity(&self) -> Identity {
        Identity::parse_or_hash(&self.identity)
    }
}
