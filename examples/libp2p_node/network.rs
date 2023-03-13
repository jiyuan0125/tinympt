//! 此模块的代码是为使用 libp2p::request_response 定制的，因此我们需要定义一个新的协议和一个编解码器
use std::io::{self, Error, ErrorKind};

use async_trait::async_trait;
use bytes::BytesMut;
use futures::prelude::*;
use libp2p::{
    mdns,
    request_response::{self, Codec, ProtocolName},
    swarm::NetworkBehaviour,
};
use prost::Message;
use tinympt::{ProofRequest, ProofResponse};
use tokio_util::{
    codec::{FramedRead, FramedWrite, LengthDelimitedCodec},
    compat::{FuturesAsyncReadCompatExt, FuturesAsyncWriteCompatExt},
};

/// 为 libp2p::request_response 定义一个新的协议
#[derive(Debug, Clone)]
pub struct ProofProtocol();

/// 实现 libp2p::request_response::ProtocolName
impl ProtocolName for ProofProtocol {
    fn protocol_name(&self) -> &[u8] {
        // 这是协议名字和版本号, libp2p 会根据这个名字来区分不同的协议
        "/proof/1".as_bytes()
    }
}

/// 为 libp2p::request_response 定义一个编解码器
#[derive(Clone)]
pub struct ProofCodec();

/// 实现 libp2p::request_response::Codec
#[async_trait]
impl Codec for ProofCodec {
    /// 协议类型
    type Protocol = ProofProtocol;
    /// Request 类型
    type Request = ProofRequest;
    /// Response 类型
    type Response = ProofResponse;

    /// 从 io 里读取一个请求。
    /// 这里的 io 是多路复用后的 stream, 下同
    async fn read_request<T>(&mut self, _: &ProofProtocol, io: &mut T) -> io::Result<Self::Request>
    where
        T: AsyncRead + Unpin + Send,
    {
        // 由于 Codec trait 里使用的 io 是 futures 里的， FramedRead 里使用的 io 是 tokio 里的，所以需要转换一下
        // 需要引入 FuturesAsyncReadCompatExt trait，这样 io 就具有了 compat 方法
        // LengthDelimitedCodec::new() 是 tokio_util 里的，用于解析帧
        let mut reader = FramedRead::new(io.compat(), LengthDelimitedCodec::new());
        // 从 reader 里读取一个帧，如果读取成功，就返回帧的内容，否则返回 None
        if let Some(buf) = reader.try_next().await? {
            // 从 buf 里解码出 ProofRequest
            // decode 方法是 prost 里的，用于解码
            ProofRequest::decode(buf).map_err(|_| Error::from(ErrorKind::UnexpectedEof))
        } else {
            Err(Error::from(ErrorKind::UnexpectedEof))
        }
    }

    /// 从 io 里读取一个响应
    async fn read_response<T>(
        &mut self,
        _: &ProofProtocol,
        io: &mut T,
    ) -> io::Result<Self::Response>
    where
        T: AsyncRead + Unpin + Send,
    {
        let mut reader = FramedRead::new(io.compat(), LengthDelimitedCodec::new());
        if let Some(buf) = reader.try_next().await? {
            ProofResponse::decode(buf).map_err(|_| Error::from(ErrorKind::UnexpectedEof))
        } else {
            Err(Error::from(ErrorKind::UnexpectedEof))
        }
    }

    /// 将一个请求写入 io
    async fn write_request<T>(
        &mut self,
        _: &ProofProtocol,
        io: &mut T,
        request: ProofRequest,
    ) -> io::Result<()>
    where
        T: AsyncWrite + Unpin + Send,
    {
        let mut writer = FramedWrite::new(io.compat_write(), LengthDelimitedCodec::new());
        let mut buf = BytesMut::with_capacity(request.encoded_len());
        request.encode(&mut buf)?;
        writer.send(buf.freeze()).await?;

        Ok(())
    }

    /// 将一个响应写入 io
    async fn write_response<T>(
        &mut self,
        _: &ProofProtocol,
        io: &mut T,
        response: ProofResponse,
    ) -> io::Result<()>
    where
        T: AsyncWrite + Unpin + Send,
    {
        let mut writer = FramedWrite::new(io.compat_write(), LengthDelimitedCodec::new());
        let mut buf = BytesMut::with_capacity(response.encoded_len());
        response.encode(&mut buf)?;
        writer.send(buf.freeze()).await?;

        Ok(())
    }
}

/// 一个组合的 NetworkBehaviour
/// 每个 behaviour 都是一个 NetworkBehaviour，然后组合成一个 ComposedBehaviour
/// NetworkBehaviour 类似于 substrate 里的 pallet，用于组装出你要的功能，也可以开发自己的 NetworkBehaviour
/// `rust-libp2p` 的作者是 "Parity Technologies <admin@parity.io>"
#[derive(NetworkBehaviour)]
#[behaviour(out_event = "ComposedEvent")]
pub struct ComposedBehaviour {
    pub proof: request_response::Behaviour<ProofCodec>,
    pub mdns: mdns::tokio::Behaviour,
}

/// 组合的事件
#[derive(Debug)]
pub enum ComposedEvent {
    Proof(request_response::Event<ProofRequest, ProofResponse>),
    Mdns(mdns::Event),
}

/// 从 request_response::Event 转换为 ComposedEvent
impl From<request_response::Event<ProofRequest, ProofResponse>> for ComposedEvent {
    fn from(value: request_response::Event<ProofRequest, ProofResponse>) -> Self {
        ComposedEvent::Proof(value)
    }
}

/// 从 mdns::Event 转换为 ComposedEvent
impl From<mdns::Event> for ComposedEvent {
    fn from(event: mdns::Event) -> Self {
        ComposedEvent::Mdns(event)
    }
}
