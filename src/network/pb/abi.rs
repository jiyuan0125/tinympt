#[derive(PartialOrd)]
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct ProofRequest {
    #[prost(bytes = "vec", tag = "1")]
    pub root_hash: ::prost::alloc::vec::Vec<u8>,
    #[prost(bytes = "vec", tag = "2")]
    pub key: ::prost::alloc::vec::Vec<u8>,
}
#[derive(PartialOrd)]
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct ProofResponse {
    #[prost(bool, tag = "1")]
    pub exists: bool,
    #[prost(bytes = "vec", tag = "2")]
    pub proof_db: ::prost::alloc::vec::Vec<u8>,
}
