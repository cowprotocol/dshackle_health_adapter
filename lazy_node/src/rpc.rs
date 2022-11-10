//! JSON RPC requests

use serde::{de, Deserialize, Deserializer, Serialize, Serializer};

pub mod eth {
    use super::*;
    use std::borrow::Cow;

    #[derive(Debug)]
    pub struct BlockNumber;

    impl Method for BlockNumber {
        type Params = [(); 0];
        type Result = u64;

        fn deserialize_result<'de, D>(deserializer: D) -> Result<Self::Result, D::Error>
        where
            D: Deserializer<'de>,
        {
            u64::from_str_radix(
                &Cow::<str>::deserialize(deserializer)?
                    .strip_prefix("0x")
                    .ok_or_else(|| de::Error::custom("missing 0x prefix"))?,
                16,
            )
            .map_err(de::Error::custom)
        }

        fn serialize_result<S>(value: &Self::Result, serializer: S) -> Result<S::Ok, S::Error>
        where
            S: Serializer,
        {
            serializer.serialize_str(&format!("{value:#x}"))
        }
    }

    impl<'de> Deserialize<'de> for BlockNumber {
        fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
        where
            D: Deserializer<'de>,
        {
            let str = Cow::<str>::deserialize(deserializer)?;
            if str != "eth_blockNumber" {
                return Err(de::Error::custom("invalid method"));
            }
            Ok(Self)
        }
    }

    impl Serialize for BlockNumber {
        fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
        where
            S: Serializer,
        {
            serializer.serialize_str("eth_blockNumber")
        }
    }
}

pub trait Method {
    type Params;
    type Result;

    fn deserialize_result<'de, D>(deserializer: D) -> Result<Self::Result, D::Error>
    where
        D: Deserializer<'de>;

    fn serialize_result<S>(value: &Self::Result, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer;
}

#[derive(Debug, Deserialize, Serialize)]
pub enum JsonRpc {
    #[serde(rename = "2.0")]
    V2,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(untagged)]
pub enum Id {
    Number(i64),
    String(String),
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Request<M>
where
    M: Method,
{
    pub jsonrpc: JsonRpc,
    pub method: M,
    pub params: M::Params,
    pub id: Id,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Response<M>
where
    M: Method,
{
    pub jsonrpc: JsonRpc,
    #[serde(
        deserialize_with = "M::deserialize_result",
        serialize_with = "M::serialize_result"
    )]
    pub result: M::Result,
    pub id: Id,
}

impl<M> Response<M>
where
    M: Method,
{
    pub fn new(request: Request<M>, result: M::Result) -> Self {
        Self {
            jsonrpc: request.jsonrpc,
            result,
            id: request.id,
        }
    }
}
