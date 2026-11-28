//! IPC 边界 u64 兼容层：把 u64 以十进制字符串序列化 / 反序列化。
//!
//! # 背景
//!
//! JavaScript 的 `number` 只有 2^53 - 1 的安全整数精度。PDF 对象标识由
//! `(xref, generation, stream_index, operator_index)` 确定性派生（规范 08 / R2.0），
//! 可达 19 位 u64；经 Tauri IPC 的 JSON 通道传成 JS `number` 会丢精度，
//! 回传 Rust 后查不到对象。
//!
//! # 职责边界
//!
//! 本模块**只在序列化边界**做表示转换：Core 内部仍用 u64 原值参与哈希、查找与比较，
//! 不修改对象 id 生成算法，不引入计数器 / 随机源，不改变任何执行链。
//!
//! # 用法
//!
//! ```ignore
//! #[derive(serde::Serialize)]
//! struct HitInfo {
//!     #[serde(with = "dw_core::serde_util::u64_as_string")]
//!     pub object_id: u64,
//! }
//! ```

/// 单个 `u64`：序列化为十进制字符串，`null` 不参与。
pub mod u64_as_string {
    use serde::{de::Error as DeError, Deserialize, Deserializer, Serializer};

    /// 把 u64 写成十进制字符串。
    pub fn serialize<S: Serializer>(value: &u64, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&value.to_string())
    }

    /// 从十进制字符串解析回 u64；非法内容返回可读错误。
    pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<u64, D::Error> {
        let text = String::deserialize(deserializer)?;
        text.parse::<u64>()
            .map_err(|error| D::Error::custom(format!("u64 字符串解析失败：{}（{}）", text, error)))
    }
}

/// 可选 `Option<u64>`：`None` 保持 `null`，`Some` 写成十进制字符串。
pub mod option_u64_as_string {
    use serde::{de::Error as DeError, Deserialize, Deserializer, Serializer};

    /// 把 Option<u64> 写成 null 或十进制字符串。
    pub fn serialize<S: Serializer>(
        value: &Option<u64>,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        match value {
            Some(number) => serializer.serialize_some(&number.to_string()),
            None => serializer.serialize_none(),
        }
    }

    /// 从 null 或十进制字符串解析回 Option<u64>。
    pub fn deserialize<'de, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<Option<u64>, D::Error> {
        let text = Option::<String>::deserialize(deserializer)?;
        match text {
            Some(text) => text
                .parse::<u64>()
                .map(Some)
                .map_err(|error| D::Error::custom(format!("u64 字符串解析失败：{}（{}）", text, error))),
            None => Ok(None),
        }
    }
}

/// `Vec<u64>`：整列写成十进制字符串数组。
pub mod vec_u64_as_string {
    use serde::{de::Error as DeError, Deserialize, Deserializer, Serialize, Serializer};

    /// 把 Vec<u64> 写成字符串数组。
    pub fn serialize<S: Serializer>(values: &Vec<u64>, serializer: S) -> Result<S::Ok, S::Error> {
        let texts: Vec<String> = values.iter().map(|number| number.to_string()).collect();
        texts.serialize(serializer)
    }

    /// 从字符串数组解析回 Vec<u64>。
    pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Vec<u64>, D::Error> {
        let texts = Vec::<String>::deserialize(deserializer)?;
        texts
            .into_iter()
            .map(|text| {
                text.parse::<u64>().map_err(|error| {
                    D::Error::custom(format!("u64 字符串解析失败：{}（{}）", text, error))
                })
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::{option_u64_as_string, u64_as_string, vec_u64_as_string};
    use serde::{Deserialize, Serialize};

    /// 模拟 IPC DTO：三种形态的 u64 字段同时使用属性。
    #[derive(Debug, PartialEq, Serialize, Deserialize)]
    struct Payload {
        #[serde(with = "u64_as_string")]
        id: u64,
        #[serde(with = "option_u64_as_string")]
        optional: Option<u64>,
        #[serde(with = "vec_u64_as_string")]
        ids: Vec<u64>,
    }

    /// 大于 2^53 的 19 位对象标识：JS number 会丢精度，字符串不会。
    const LARGE_ID: u64 = 1_617_674_655_699_130_457;

    #[test]
    fn large_u64_round_trips_as_decimal_string() {
        let payload = Payload {
            id: LARGE_ID,
            optional: Some(LARGE_ID),
            ids: vec![LARGE_ID, 31_774_738_392_861_546, u64::MAX],
        };
        let json = serde_json::to_string(&payload).expect("DTO 应能序列化");
        assert!(
            json.contains(&format!("\"id\":\"{}\"", LARGE_ID)),
            "u64 必须以十进制字符串出现在 JSON 中：{}",
            json
        );
        assert!(
            !json.contains(&format!("\"id\":{}", LARGE_ID)),
            "u64 不得作为 JSON 数字出现：{}",
            json
        );
        let restored: Payload = serde_json::from_str(&json).expect("字符串应能解析回 u64");
        assert_eq!(restored, payload);
    }

    #[test]
    fn none_and_empty_vec_keep_their_shape() {
        let payload = Payload {
            id: 0,
            optional: None,
            ids: Vec::new(),
        };
        let json = serde_json::to_string(&payload).expect("DTO 应能序列化");
        assert!(json.contains("\"optional\":null"), "空值应保持 null：{}", json);
        assert!(json.contains("\"ids\":[]"), "空数组应保持空数组：{}", json);
        let restored: Payload = serde_json::from_str(&json).expect("应能反序列化");
        assert_eq!(restored, payload);
    }

    #[test]
    fn invalid_digit_text_is_rejected() {
        let invalid = serde_json::from_str::<Payload>(
            r#"{"id":"不是数字","optional":null,"ids":[]}"#,
        );
        assert!(invalid.is_err(), "非十进制字符串必须报错，不能静默丢精度");
    }
}
