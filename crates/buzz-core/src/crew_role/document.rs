//! Duplicate-safe YAML decoding with the existing typed-string semantics.

use std::{collections::BTreeMap, fmt, marker::PhantomData};

use serde::{
    de::{Error, MapAccess, Visitor},
    Deserialize, Deserializer,
};
use serde_yaml::{Mapping, Value};

use super::RoleParseError;

pub(super) fn assignment_entries(document: &serde_yaml::Mapping) -> BTreeMap<String, String> {
    document
        .get(Value::String("assignments".into()))
        .and_then(Value::as_mapping)
        .into_iter()
        .flatten()
        .filter_map(|(key, value)| Some((key.as_str()?.to_string(), value.as_str()?.to_string())))
        .collect()
}

struct StringMap<T>(BTreeMap<String, T>);

impl<'de, T: Deserialize<'de>> Deserialize<'de> for StringMap<T> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct MapVisitor<T>(PhantomData<T>);
        impl<'de, T: Deserialize<'de>> Visitor<'de> for MapVisitor<T> {
            type Value = StringMap<T>;
            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("a mapping with unique string keys")
            }
            fn visit_map<M: MapAccess<'de>>(self, mut map: M) -> Result<Self::Value, M::Error> {
                let mut values = BTreeMap::new();
                while let Some((key, value)) = map.next_entry::<String, T>()? {
                    if values.insert(key, value).is_some() {
                        return Err(M::Error::custom("duplicate Crew mapping key"));
                    }
                }
                Ok(StringMap(values))
            }
        }
        deserializer.deserialize_map(MapVisitor(PhantomData))
    }
}

struct Document(Mapping);

impl<'de> Deserialize<'de> for Document {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct DocumentVisitor;
        impl<'de> Visitor<'de> for DocumentVisitor {
            type Value = Document;
            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("a Crew mapping")
            }
            fn visit_map<M: MapAccess<'de>>(self, mut map: M) -> Result<Self::Value, M::Error> {
                let mut root = Mapping::new();
                while let Some(key) = map.next_key::<Value>()? {
                    if root.contains_key(&key) {
                        return Err(M::Error::custom("duplicate Crew document key"));
                    }
                    let value = match key.as_str() {
                        Some("assignments" | "definitions" | "routing") => {
                            let typed = map.next_value::<StringMap<String>>()?;
                            serde_yaml::to_value(typed.0).map_err(M::Error::custom)?
                        }
                        Some("capabilities") => {
                            let typed = map.next_value::<StringMap<Vec<String>>>()?;
                            serde_yaml::to_value(typed.0).map_err(M::Error::custom)?
                        }
                        Some("contact") => {
                            let contact = map.next_value::<Option<String>>()?;
                            serde_yaml::to_value(contact).map_err(M::Error::custom)?
                        }
                        // Value preserves unknown semantic values and rejects
                        // duplicate keys in every unknown nested mapping.
                        _ => map.next_value::<Value>()?,
                    };
                    root.insert(key, value);
                }
                Ok(Document(root))
            }
        }
        deserializer.deserialize_map(DocumentVisitor)
    }
}

pub(super) fn parse_document(yaml: &str) -> Result<Value, RoleParseError> {
    serde_yaml::from_str::<Document>(yaml)
        .map(|document| Value::Mapping(document.0))
        .map_err(|error| RoleParseError::InvalidYaml(error.to_string()))
}
