use std::env;
use std::path::PathBuf;

use anyhow::Result;
use bytes::BytesMut;
use clap::Parser;
use futures::channel::mpsc::{self, UnboundedReceiver, UnboundedSender};
use futures::channel::oneshot;
use futures::prelude::*;
use prost::Message;
use tinympt::{ProofRequest, ProofResponse, RocksdbTrie, Trie};
use tokio::net::{TcpListener, TcpStream};
use tokio_util::codec::{Framed, LengthDelimitedCodec};

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// 服务器监听的地址
    #[arg(long, default_value = "127.0.0.1:9988")]
    server_addr: String,
    /// rocksdb 数据库的路径
    #[arg(long, default_value = "/tmp/tinympt_db")]
    db_path: PathBuf,
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

    // 创建一个无界的 channel，用于将 proof request 发送给
    let (req_sender, req_receiver) =
        mpsc::unbounded::<(ProofRequest, oneshot::Sender<ProofResponse>)>();

    // 初始化 trie
    let mut trie = RocksdbTrie::<String, String>::new(args.db_path);
    init_trie(&mut trie)?;

    // 启动 proof request 处理协程
    tokio::spawn(async move {
        if let Err(e) = process_proof_request(trie, req_receiver).await {
            log::error!("Failed to process proof request; error = {}", e);
        }
    });

    // 开始监听
    let listener = TcpListener::bind(args.server_addr).await?;
    log::info!("Listening on {}", listener.local_addr()?);

    loop {
        // 接收连接
        let (stream, remote_addr) = listener.accept().await?;
        log::info!("New connection from {}", remote_addr);

        // 将 req_sender 克隆一份，用于传递给 stream 处理协程
        let req_sender = req_sender.clone();
        // 启动一个协程处理 stream
        tokio::spawn(async move {
            if let Err(e) = process_stream(stream, req_sender).await {
                log::error!("Failed to process connection; error = {}", e);
            }
        });
    }
}

/// 初始化 trie
fn init_trie(trie: &mut RocksdbTrie<String, String>) -> Result<()> {
    let data = [
        (
            "pellet01_state01_key01".to_string(),
            "pellet01_state01_value01".to_string(),
        ),
        (
            "pellet01_state01_key02".to_string(),
            "pellet01_state01_value02".to_string(),
        ),
        (
            "pellet01_state02_key01".to_string(),
            "pellet01_state02_value01".to_string(),
        ),
        (
            "pellet01_state02_key02".to_string(),
            "pellet01_state02_value02".to_string(),
        ),
        (
            "pellet02_state01_key01".to_string(),
            "pellet02_state01_value01".to_string(),
        ),
        (
            "pellet02_state01_key02".to_string(),
            "pellet02_state01_value02".to_string(),
        ),
    ];

    for (key, value) in data.into_iter() {
        trie.insert(key, value)?;
    }

    let root_hash = trie
        .commit()
        .expect("Failed to commit trie")
        .expect("root hash is None");

    log::info!("Root hash = {:?}", hex::encode(root_hash));
    Ok(())
}

/// stream 处理函数
async fn process_stream(
    stream: TcpStream,
    mut req_sender: UnboundedSender<(ProofRequest, oneshot::Sender<ProofResponse>)>,
) -> Result<()> {
    // 构建framed，framed 在发送时将要发送的数据封装成帧，每个帧头是一个表示数据长度的u32，读取时将参考帧头来确报读到一个完整帧
    let mut framed = Framed::new(stream, LengthDelimitedCodec::new());
    let mut buf = BytesMut::new();

    // 读取服务端响应，framed 会自动将数据帧解析出来，如果未出错，这里收到的 bytes 包含了一个请求的完整数据，请放心解析
    while let Some(bytes) = framed.try_next().await? {
        let (res_sender, res_receiver) = oneshot::channel();
        // 反序列化出 proof request
        let proof_request = ProofRequest::decode(bytes)?;
        // 将 proof request 连同 res_sender 发送给 proof request 处理协程
        req_sender.send((proof_request, res_sender)).await?;
        // 等待 proof request 处理协程处理完毕
        let proof_response = res_receiver.await?;
        // 将 proof response 发送给客户端
        proof_response.encode(&mut buf)?;
        framed.send(buf.split().freeze()).await?;
    }
    Ok(())
}

/// 处理 req_receiver 中的请求
async fn process_proof_request(
    mut trie: RocksdbTrie<String, String>,
    mut req_receiver: UnboundedReceiver<(ProofRequest, oneshot::Sender<ProofResponse>)>,
) -> Result<()> {
    // 从 req_receiver 中获取请求，然后处理请求，将结果发送回去。
    // 注意，while 循环退出时，req_receiver 会被 drop, 导致服务端无法正常工作。
    // 所以我们在 while 内部处理所有的错误，以免因为错误发生时导致 while 退出。
    while let Some((proof_request, res_sender)) = req_receiver.next().await {
        // 从 proof_request 中获取 hash_value 和 key
        let (hash_value, key) = match proof_request.try_into() {
            Ok((hash_value, key)) => (hash_value, key),
            Err(e) => {
                log::error!("Failed to convert proof request; error = {}", e);
                continue;
            }
        };
        // 从 trie 中获取 proof
        let proof = match trie.get_proof(&hash_value, &key) {
            Ok(proof) => proof,
            Err(e) => {
                log::error!("Failed to get proof; error = {}", e);
                continue;
            }
        };
        // 将 proof 转换为 proof_response
        let proof_response = match proof.try_into() {
            Ok(proof_response) => proof_response,
            Err(e) => {
                log::error!("Failed to convert proof; error = {}", e);
                continue;
            }
        };
        // 将 proof_response 发送回去
        let _ = res_sender.send(proof_response);
    }
    Ok(())
}
