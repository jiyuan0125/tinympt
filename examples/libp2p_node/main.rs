use anyhow::Result;
use std::{collections::HashSet, env, iter, path::PathBuf};
use tinympt::{self, ProofRequest, ProofResponse, RocksdbTrie, Trie, TrieError};
use tokio::sync::oneshot;

use clap::Parser;
use futures::{
    channel::mpsc::{self, UnboundedReceiver},
    prelude::*,
};
use libp2p::{
    identity::{ed25519::SecretKey, Keypair},
    mdns,
    request_response::{self, Event, Message, ProtocolSupport},
    swarm::SwarmEvent,
    tokio_development_transport, Multiaddr, PeerId, Swarm,
};
use network::{ComposedBehaviour, ProofCodec, ProofProtocol};

use crate::network::ComposedEvent;

mod network;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// 用于生成公私钥的种子
    #[clap(long)]
    secret_key_seed: Option<u8>,
    /// 需要拨号的节点 Multiaddr
    #[clap(long)]
    to_dial: Option<String>,
    /// rocksdb 数据库的路径
    #[clap(long)]
    db_path: PathBuf,
    /// 构建 proof 请求时使用的根哈希
    #[arg(
        long,
        default_value = "1b18217ad8a87e1accfdf7b3b1c4573985c932b711d6494db246e59fb884e952"
    )]
    root_hash: String,
    #[arg(long, default_value = "pellet02_state01_key02")]
    /// 构建 proof 请求时使用的 key
    key: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    // 设置 log 级别
    if let Err(_) = env::var("RUST_LOG") {
        env::set_var("RUST_LOG", "info");
    }

    // 日志初始化
    env_logger::init();
    // 解析命令行参数
    let args = Args::parse();

    // 定义一个 channle 用于接收请求
    let (mut req_sender, req_receiver) =
        mpsc::unbounded::<(ProofRequest, oneshot::Sender<ProofResponse>)>();

    // 构建一个 RocksdbTrie
    let mut trie = RocksdbTrie::<String, String>::new(args.db_path);
    // 初始化 trie
    init_trie(&mut trie)?;

    // 开启一个协程，处理 req_receiver 中的请求
    tokio::spawn(async move {
        if let Err(e) = process_proof_request(trie, req_receiver).await {
            log::error!("Failed to process proof request; error = {}", e);
        }
    });

    // 生成 ed25519 公私钥对，如果提供了种子，就用种子生成
    let keypair = match args.secret_key_seed {
        Some(seed) => {
            let mut bytes = [0u8; 32];
            bytes[0] = seed;
            let secret_key = SecretKey::from_bytes(&mut bytes)
                .expect("Only occur when the length is incorrect, no problem here.");
            Keypair::Ed25519(secret_key.into())
        }
        None => Keypair::generate_ed25519(),
    };

    // 用公钥生成本地节点的 peer_id
    let local_peer_id = keypair.public().to_peer_id();

    // 构造一个 swarm，参数是 trasport, behaviour, peer_id
    // tokio_development_transport 返回一个支持 tcp, ws, dns, noise, mplex, yamux 的 transport
    // 使用 ComposedBehaviour 作为 behaviour, 组装了两个行为，request_response 和 mdns
    let mut swarm = Swarm::with_tokio_executor(
        tokio_development_transport(keypair.clone()).unwrap(),
        ComposedBehaviour {
            proof: request_response::Behaviour::new(
                ProofCodec(),
                iter::once((ProofProtocol(), ProtocolSupport::Full)),
                Default::default(),
            ),
            mdns: mdns::Behaviour::new(Default::default(), local_peer_id)?,
        },
        local_peer_id,
    );

    // 如果参数里指定了要拨号的节点，咱就拨号主动联系一下
    if let Some(to_dial) = args.to_dial {
        let addr: Multiaddr = to_dial.parse()?;
        swarm.dial(addr)?;
    }

    // Listen on all interfaces and whatever port the OS assigns
    // 0.0.0.0 指监听所有的 interface，最后的 0 指的是由操作系统随机分配一个端口
    swarm.listen_on("/ip4/0.0.0.0/tcp/0".parse()?)?;

    // 用户输入的 root_hash 是个 hex 字符串，需要转换成 bytes 数组
    // 无法保证用户的输入能正确转换成[u8;32], 所以可能有错误发生
    let root_hash = hex::decode(args.root_hash)?
        .try_into()
        .map_err(|_| TrieError::InvalidHashValue)?;

    // 从用户输入的数据转换成一个 proof_request
    let proof_request = ProofRequest::from((root_hash, args.key.clone()));

    // 用来存储已经建立连接的 peer_id
    let mut peer_ids: HashSet<PeerId> = HashSet::new();

    // 这里的 loop 是一个无限循环，每次循环都会从 swarm 中获取一个事件
    loop {
        tokio::select! {
            // 从 swarm 中获取事件
            // match event 这段代码可以封装成一个函数
            // 在 select! 里直接写代码，无法使用代码自动格式化功能
            event = swarm.select_next_some() => match event {
                // 当有新的地址被监听时，打印出来
                SwarmEvent::NewListenAddr { address, .. } => {
                    log::info!("Listening on {address:?}");
                }
                // 这里算是所有逻辑的入口, 我将处理步骤在从这里开始编号
                // 1、当有新的节点被发现时，尝试连接
                SwarmEvent::Behaviour(ComposedEvent::Mdns(event)) => {
                    match event {
                        mdns::Event::Discovered(list) => {
                            for (_peer_id, multiaddr) in list {
                                swarm.dial(multiaddr)?;
                            }
                        }
                        _ => {}
                    }
                }
                // 2、当有新的连接建立时，发送 proof_request, 这里节点相当于客户端身份
                SwarmEvent::ConnectionEstablished { peer_id, .. } => {
                    // 使用 peer_ids 来确保只发送一次 proof_request
                    if !peer_ids.contains(&peer_id) {
                        log::info!("Connection established with {peer_id:?}");
                        let proof_request = proof_request.clone();
                        // 向服务端身份的节点发送请求
                        swarm.behaviour_mut().proof.send_request(&peer_id, proof_request);
                        // 将 peer_id 加入到 peer_ids 中
                        peer_ids.insert(peer_id);
                    }
                }
                // 当有新的消息到达时，这里的消息存在两种，分别是收到请求的消息和收到响应的消息
                SwarmEvent::Behaviour(ComposedEvent::Proof(Event::Message {
                    message,
                    ..
                })) => match message {
                    // 3、当收到请求时，处理请求, 此时的节点是服务端身份
                    Message::Request { channel, request: proof_request, .. } => {
                        // 定义一个 oneshot 通道，用来将处理结果
                        let (res_sender, res_receiver) = oneshot::channel();
                        // 将 proof_requset 连同 oneshot 一同发送
                        req_sender.send((proof_request, res_sender)).await?;
                        // 等待处理结果
                        let proof_response = res_receiver.await?;
                        // 将处理结果发送给客户端身份的节点
                        let _ =
                            swarm
                            .behaviour_mut()
                            .proof
                            .send_response(channel, proof_response);
                    }
                    // 4、当收到响应时，处理响应，此时的节点是客户端身份
                    Message::Response {
                        response: proof_response,
                        ..
                    } => {
                        log::info!("Proof response, exists = {}", proof_response.exists);
                        // 将 proof_response 转换成 (bool, Vec<u8>)
                        let (exists, proof_db) = proof_response.try_into()?;
                        if exists {
                            // 验证 proof, Some(value) 表示验证成功，None 表示验证失败
                            let value: Option<String> =
                                tinympt::verify_proof(&root_hash, &proof_db, &args.key)?;

                            log::info!("Value = {:?}", value);
                        }
                    }
                }
                _ => {}
            }
        }
    }
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

/// 为 trie 初始化数据
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
