use std::env;

use anyhow::Result;
use bytes::BytesMut;
use clap::Parser;
use futures::prelude::*;
use prost::Message;
use tinympt::{verify_proof, ProofRequest, ProofResponse, TrieError};
use tokio::net::TcpStream;
use tokio_util::codec::{Framed, LengthDelimitedCodec};

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// 服务器的地址
    #[arg(long, default_value = "127.0.0.1:9988")]
    server_addr: String,
    #[arg(
        long,
        default_value = "1b18217ad8a87e1accfdf7b3b1c4573985c932b711d6494db246e59fb884e952"
    )]
    /// 构建 proof 请求时使用的 root hash
    root_hash: String,
    /// 构建 proof 请求时使用的 key
    #[arg(long, default_value = "pellet02_state01_key02")]
    key: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    // 设置日志级别
    if let Err(_) = env::var("RUST_LOG") {
        env::set_var("RUST_LOG", "info");
    }

    // 初始化 env_logger
    env_logger::init();
    // 解析命令行参数
    let args = Args::parse();

    // 连接到服务器
    let stream = TcpStream::connect(args.server_addr).await?;
    // 解析用户输入的 root hash，如果解析失败则返回错误
    let root_hash = hex::decode(args.root_hash)?
        .try_into()
        .map_err(|_| TrieError::InvalidHashValue)?;
    // 构建 proof 请求
    let proof_request = { ProofRequest::from((root_hash, args.key.clone())) };
    // 序列号 proof request
    let mut buf = BytesMut::new();
    proof_request.encode(&mut buf)?;
    // 构建framed，framed 在发送时将要发送的数据封装成帧，每个帧头是一个表示数据长度的u32，读取时将参考帧头来确报读到一个完整帧
    let mut framed = Framed::new(stream, LengthDelimitedCodec::new());
    // 将数据帧发送给服务端
    framed.send(buf.split().freeze()).await?;
    // 读取服务端响应，framed 会自动将数据帧解析出来，如果未出错，这里收到的 bytes 包含了一个响应的完整数据，请放心解析
    if let Some(bytes) = framed.try_next().await? {
        // 反序列化出 proof response
        let proof_response = ProofResponse::decode(bytes)?;
        log::info!("Proof response, exists = {}", proof_response.exists);
        // 将 proof response 转换成 (bool, Vec<u8>)，如果转换失败则返回错误
        let (exists, proof_db) = proof_response.try_into()?;
        if exists {
            // 验证 proof, Some(value) 表示验证成功，None 表示验证失败
            let value: Option<String> =
                verify_proof(&root_hash, &proof_db, &args.key)?;

            log::info!("Value = {:?}", value);
        }
    }

    Ok(())
}
