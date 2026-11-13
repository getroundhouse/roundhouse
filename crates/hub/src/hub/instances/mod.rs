//! The instances the hub asks its providers for, each recorded before it asks, as `store::instances` says.

#[cfg(test)]
use super::Hub;
#[cfg(test)]
use protocol::model::InstanceId;
#[cfg(test)]
use providers::Provider;
#[cfg(test)]
use std::sync::Arc;

#[cfg(test)]
mod tests;

mod instance_calls;
