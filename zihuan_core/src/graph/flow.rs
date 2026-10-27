use std::collections::HashMap;
use std::ops::{Deref, DerefMut};

use crate::error::{Error, Result};

use crate::graph::DataValue;

type FlowValues = HashMap<String, DataValue>;

macro_rules! define_value_flow {
    ($name:ident, $missing_label:literal) => {
        #[derive(Debug, Clone, Default)]
        pub struct $name {
            values: FlowValues,
        }

        impl $name {
            pub fn new() -> Self {
                Self::default()
            }

            pub fn get(&self, key: &str) -> Option<&DataValue> {
                self.values.get(key)
            }

            pub fn get_required(&self, key: &str) -> Result<&DataValue> {
                self.values
                    .get(key)
                    .ok_or_else(|| Error::ValidationError(format!($missing_label, key)))
            }

            pub fn get_optional(&self, key: &str) -> Option<&DataValue> {
                self.get(key)
            }

            pub fn contains(&self, key: &str) -> bool {
                self.values.contains_key(key)
            }

            pub fn insert(
                &mut self,
                key: impl Into<String>,
                value: DataValue,
            ) -> Option<DataValue> {
                self.values.insert(key.into(), value)
            }

            pub fn iter(&self) -> impl Iterator<Item = (&String, &DataValue)> {
                self.values.iter()
            }

            pub fn into_inner(self) -> FlowValues {
                self.values
            }

            pub fn as_map(&self) -> &FlowValues {
                &self.values
            }
        }

        impl From<FlowValues> for $name {
            fn from(values: FlowValues) -> Self {
                Self { values }
            }
        }

        impl From<$name> for FlowValues {
            fn from(flow: $name) -> Self {
                flow.values
            }
        }

        impl Deref for $name {
            type Target = FlowValues;

            fn deref(&self) -> &Self::Target {
                &self.values
            }
        }
    };
}
define_value_flow!(NodeInputFlow, "Required input port '{}' is missing");
define_value_flow!(NodeOutputFlow, "Required output port '{}' is missing");
define_value_flow!(NodeConfigFlow, "Required config field '{}' is missing");
define_value_flow!(RuntimeValueFlow, "Required runtime value '{}' is missing");

impl DerefMut for NodeOutputFlow {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.values
    }
}

impl IntoIterator for NodeOutputFlow {
    type Item = (String, DataValue);
    type IntoIter = std::collections::hash_map::IntoIter<String, DataValue>;

    fn into_iter(self) -> Self::IntoIter {
        self.values.into_iter()
    }
}

impl DerefMut for NodeConfigFlow {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.values
    }
}

impl DerefMut for RuntimeValueFlow {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.values
    }
}

impl IntoIterator for NodeConfigFlow {
    type Item = (String, DataValue);
    type IntoIter = std::collections::hash_map::IntoIter<String, DataValue>;

    fn into_iter(self) -> Self::IntoIter {
        self.values.into_iter()
    }
}

impl IntoIterator for RuntimeValueFlow {
    type Item = (String, DataValue);
    type IntoIter = std::collections::hash_map::IntoIter<String, DataValue>;

    fn into_iter(self) -> Self::IntoIter {
        self.values.into_iter()
    }
}
