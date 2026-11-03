use std::{collections::BTreeMap, fmt};

use serde::{Deserialize, Deserializer, de};

struct Catalog(BTreeMap<String, String>);

impl<'de> Deserialize<'de> for Catalog {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct Visitor;
        impl<'de> de::Visitor<'de> for Visitor {
            type Value = Catalog;

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("不含重复键的平面字符串词典")
            }

            fn visit_map<M: de::MapAccess<'de>>(self, mut map: M) -> Result<Catalog, M::Error> {
                let mut catalog = BTreeMap::new();
                while let Some((key, value)) = map.next_entry::<String, String>()? {
                    match catalog.entry(key) {
                        std::collections::btree_map::Entry::Occupied(entry) => {
                            return Err(de::Error::custom(format!("重复消息键：{}", entry.key())));
                        }
                        std::collections::btree_map::Entry::Vacant(entry) => {
                            entry.insert(value);
                        }
                    }
                }
                Ok(Catalog(catalog))
            }
        }
        deserializer.deserialize_map(Visitor)
    }
}

pub(super) fn messages<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<BTreeMap<String, BTreeMap<String, String>>, D::Error> {
    struct Visitor;
    impl<'de> de::Visitor<'de> for Visitor {
        type Value = BTreeMap<String, BTreeMap<String, String>>;

        fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            formatter.write_str("语言标识不重复的消息词典")
        }

        fn visit_map<M: de::MapAccess<'de>>(self, mut map: M) -> Result<Self::Value, M::Error> {
            let mut messages = BTreeMap::new();
            while let Some((locale, Catalog(catalog))) = map.next_entry::<String, Catalog>()? {
                match messages.entry(locale) {
                    std::collections::btree_map::Entry::Occupied(entry) => {
                        return Err(de::Error::custom(format!("重复词典语言：{}", entry.key())));
                    }
                    std::collections::btree_map::Entry::Vacant(entry) => {
                        entry.insert(catalog);
                    }
                }
            }
            Ok(messages)
        }
    }
    deserializer.deserialize_map(Visitor)
}
