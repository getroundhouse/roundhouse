//! Which sandbox is which instance, of which hub, since every sandbox of the user's is in one list.

use protocol::model::{HubId, InstanceId};

/// The label every sandbox the adapter makes carries, whose value is the hub it's for.
pub(crate) const HUB_LABEL: &str = "roundhouse.hub";

/// The hub's sandboxes, by name.
pub(crate) struct Names {
    /// What every one of the hub's sandboxes' names starts with.
    prefix: String,
}

impl Names {
    pub(crate) fn of(hub_id: &HubId) -> Self {
        Self { prefix: format!("rh-{hub_id}-") }
    }

    /// The sandbox that's `instance_id`. Both are UUIDs as the hub makes them, which a sandbox's name can hold.
    pub(crate) fn sandbox(&self, instance_id: &InstanceId) -> String {
        format!("{}{instance_id}", self.prefix)
    }

    /// The instance the sandbox `name` is, if it's one of the hub's.
    pub(crate) fn instance(&self, name: &str) -> Option<InstanceId> {
        name.strip_prefix(&self.prefix).filter(|id| !id.is_empty()).map(InstanceId::from)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// So the adapter lists only the hub's own instances, and finds each by the ID the hub gave it.
    #[test]
    fn a_sandbox_is_named_for_its_hub_and_instance_and_read_back_as_the_instance() {
        let names = Names::of(&"7f1c".into());
        let instance = InstanceId::from("9a0b-41");

        assert_eq!(names.sandbox(&instance), "rh-7f1c-9a0b-41");
        assert_eq!(names.instance(&names.sandbox(&instance)), Some(instance));
        assert_eq!(names.instance("rh-other-9a0b-41"), None, "another hub's");
        assert_eq!(names.instance("rh-7f1c-"), None);
    }
}
