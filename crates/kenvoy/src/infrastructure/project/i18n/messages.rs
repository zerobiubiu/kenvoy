use std::{collections::BTreeMap, fmt};

use serde::{Deserialize, Deserializer, de};

struct Messages(BTreeMap<String, String>);

impl<'de> Deserialize<'de> for Messages {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct Visitor;
        impl<'de> de::Visitor<'de> for Visitor {
            type Value = Messages;

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("平面 JSON 对象，消息键和值均为字符串且键不重复")
            }

            fn visit_map<M: de::MapAccess<'de>>(self, mut map: M) -> Result<Messages, M::Error> {
                let mut messages = BTreeMap::new();
                while let Some((key, value)) = map.next_entry::<String, String>()? {
                    if key.is_empty() {
                        return Err(de::Error::custom("消息键不能为空"));
                    }
                    match messages.entry(key) {
                        std::collections::btree_map::Entry::Occupied(entry) => {
                            return Err(de::Error::custom(format!("重复消息键：{}", entry.key())));
                        }
                        std::collections::btree_map::Entry::Vacant(entry) => {
                            entry.insert(value);
                        }
                    }
                }
                Ok(Messages(messages))
            }
        }
        deserializer.deserialize_map(Visitor)
    }
}

pub(super) fn parse(source: &str) -> Result<BTreeMap<String, String>, serde_json::Error> {
    serde_json::from_str::<Messages>(source).map(|messages| messages.0)
}
