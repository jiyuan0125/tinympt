use blake2::digest::{Update, VariableOutput};
use blake2::Blake2bVar;

use crate::{NibbleSlice, NibbleVec};

/// 将 &[u8] 转换为 NibbleVec
pub fn convert_bytes_to_nibbles(bytes: &[u8]) -> NibbleVec {
    let mut nibbles = Vec::with_capacity(bytes.len() * 2);
    for byte in bytes {
        // 将高 4 位放入 nibbles
        nibbles.push(byte >> 4);
        // 将低 4 位放入 nibbles
        nibbles.push(byte & 0x0f);
    }
    nibbles
}

/// 获得两个 NibbleSlice 的共同前缀， 并返回(共同前缀, n1去掉共同前缀的剩余部分, n2去掉共同前缀的剩余部分)
pub fn parse_nibble_slices_shared_portion<'a, 'b>(
    n1: &'a NibbleSlice,
    n2: &'b NibbleSlice,
) -> (&'a NibbleSlice, &'a NibbleSlice, &'b NibbleSlice) {
    let min = n1.len().min(n2.len());

    // 因为这段逻辑要使用两次，定义一个闭包。
    // 也可以定义一个函数，其实定义一个函数更适合，因为这个闭包没有捕获环境中的变量。
    let split = |i| {
        let shared = &n1[0..i];
        let r1 = &n1[i..];
        let r2 = &n2[i..];

        (shared, r1, r2)
    };

    // 循环比对两个 NibbleSlice 的每一个 nibble
    for i in 0..min {
        if n1[i] != n2[i] {
            return split(i);
        }
    }

    return split(min);
}

/// 获得 &[u8] 的哈希
pub fn hash(data: &[u8]) -> [u8; 32] {
    // 构建 hasher
    let mut hasher = Blake2bVar::new(32).unwrap();
    // 更新 hasher
    hasher.update(data);
    // 计算哈希
    let mut buf = [0u8; 32];
    hasher.finalize_variable(&mut buf).unwrap();
    buf
}

#[cfg(test)]
mod util_test {
    use super::*;

    #[test]
    fn parse_nibble_slices_shared_portion_works() {
        let n1 = &[0x01, 0x02, 0x03][..];
        let n2 = &[0x01, 0x02, 0x03, 0x04][..];

        let expeced_shared = &[0x01, 0x02, 0x03];
        let expeced_r1 = &[];
        let expeced_r2 = &[0x04];

        let (shared, r1, r2) = parse_nibble_slices_shared_portion(&n1, &n2);

        assert_eq!(shared, expeced_shared);
        assert_eq!(r1, expeced_r1);
        assert_eq!(r2, expeced_r2);
    }
}
