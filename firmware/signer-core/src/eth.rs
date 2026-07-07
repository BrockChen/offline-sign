//! ETH 签名（k256，纯 Rust）与最小 EIP-1559 解码（供设备屏幕核对）。
//!
//! 签名：`keccak256(sign_data)` → k256 可恢复签名 → 65 字节 `r‖s‖v`（v = y-parity 0/1，
//! 与 x86 版 `as_rsy()` 一致）。解码：手写最小 RLP 解析器提取交易字段（不依赖 alloy）。

use k256::ecdsa::{RecoveryId, Signature, SigningKey};
use sha3::{Digest, Keccak256};

use crate::airgap::eth::{DataType, EthSignRequest};
use crate::{derive, Error, Result};

fn keccak256(data: &[u8]) -> [u8; 32] {
    let mut out = [0u8; 32];
    out.copy_from_slice(&Keccak256::digest(data));
    out
}

/// 对 `sign_data` 用 `m/44'/60'/account'/0/index` 的私钥签名，返回 65 字节 `r‖s‖v`。
pub fn sign_sign_data(seed: &[u8], account: u32, index: u32, sign_data: &[u8]) -> Result<[u8; 65]> {
    let sk_bytes = derive::eth_privkey(seed, account, index)?;
    let sk = SigningKey::from_slice(&sk_bytes).map_err(|e| Error::Sign(format!("{e}")))?;
    let hash = keccak256(sign_data);
    let (sig, recid): (Signature, RecoveryId) = sk
        .sign_prehash_recoverable(&hash)
        .map_err(|e| Error::Sign(format!("{e}")))?;
    let mut out = [0u8; 65];
    out[..64].copy_from_slice(&sig.to_bytes());
    out[64] = recid.to_byte(); // 0/1 y-parity
    Ok(out)
}

/// 处理一个 eth-sign-request：按其派生路径签名，返回 (request_id, 65 字节签名)。
/// 若带 address，可由上层再核对；此处仅按路径签。
pub fn sign_request(seed: &[u8], req: &EthSignRequest) -> Result<(Vec<u8>, [u8; 65])> {
    let (account, index) = req
        .derivation
        .eth_account_index()
        .ok_or_else(|| Error::Protocol("派生路径不是标准 ETH 路径 m/44'/60'/a'/0/i".into()))?;
    let sig = sign_sign_data(seed, account, index, &req.sign_data)?;
    Ok((req.request_id.clone(), sig))
}

/// EIP-1559 交易的屏幕核对摘要（值以 wei 计）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EthSummary {
    pub chain_id: u64,
    pub nonce: u64,
    pub to: Option<[u8; 20]>,
    pub value_wei: u128,
    pub gas_limit: u64,
    pub max_fee_per_gas: u128,
    pub max_priority_fee_per_gas: u128,
    pub data_len: usize,
}

/// 从 eth-sign-request 解出摘要（仅支持 data-type=typed-transaction 的 EIP-1559）。
pub fn summarize(req: &EthSignRequest) -> Result<EthSummary> {
    match req.data_type {
        DataType::TypedTransaction => decode_eip1559(&req.sign_data),
        other => Err(Error::Protocol(format!(
            "暂不支持解码 data-type {other:?} 做核对（拒绝盲签）"
        ))),
    }
}

/// 解析 `0x02 || rlp(未签名 EIP-1559 字段)` 为屏幕核对摘要。
pub fn decode_eip1559(sign_data: &[u8]) -> Result<EthSummary> {
    let first = *sign_data
        .first()
        .ok_or_else(|| Error::Protocol("sign-data 为空".into()))?;
    if first != 0x02 {
        return Err(Error::Protocol(format!(
            "期望 EIP-1559 类型前缀 0x02，实际 0x{first:02x}"
        )));
    }
    let mut r = Rlp::new(&sign_data[1..]);
    r.list_header()?; // 进入 9 元素列表
    let chain_id = be_u64(r.str()?);
    let nonce = be_u64(r.str()?);
    let max_priority_fee_per_gas = be_u128(r.str()?);
    let max_fee_per_gas = be_u128(r.str()?);
    let gas_limit = be_u64(r.str()?);
    let to_bytes = r.str()?;
    let value_wei = be_u128(r.str()?);
    let data = r.str()?;
    // accessList（列表）无需解析。
    let to = if to_bytes.len() == 20 {
        let mut a = [0u8; 20];
        a.copy_from_slice(to_bytes);
        Some(a)
    } else {
        None
    };
    Ok(EthSummary {
        chain_id,
        nonce,
        to,
        value_wei,
        gas_limit,
        max_fee_per_gas,
        max_priority_fee_per_gas,
        data_len: data.len(),
    })
}

fn be_u64(b: &[u8]) -> u64 {
    let mut v = 0u64;
    for &x in b {
        v = (v << 8) | x as u64;
    }
    v
}
fn be_u128(b: &[u8]) -> u128 {
    let mut v = 0u128;
    for &x in b {
        v = (v << 8) | x as u128;
    }
    v
}

/// 最小 RLP 读取器（够解 EIP-1559 的字符串字段与列表头）。
struct Rlp<'a> {
    b: &'a [u8],
    i: usize,
}

impl<'a> Rlp<'a> {
    fn new(b: &'a [u8]) -> Self {
        Rlp { b, i: 0 }
    }
    fn short() -> Error {
        Error::Protocol("RLP 数据过短".into())
    }
    fn take(&mut self, n: usize) -> Result<&'a [u8]> {
        let end = self.i.checked_add(n).ok_or_else(Self::short)?;
        let s = self.b.get(self.i..end).ok_or_else(Self::short)?;
        self.i = end;
        Ok(s)
    }
    /// 读一个字符串项，返回其 payload。
    fn str(&mut self) -> Result<&'a [u8]> {
        let b0 = *self.b.get(self.i).ok_or_else(Self::short)?;
        if b0 <= 0x7f {
            self.i += 1;
            Ok(&self.b[self.i - 1..self.i])
        } else if b0 <= 0xb7 {
            self.i += 1;
            self.take((b0 - 0x80) as usize)
        } else if b0 <= 0xbf {
            self.i += 1;
            let ll = (b0 - 0xb7) as usize;
            let lb = self.take(ll)?;
            let len = be_u64(lb) as usize;
            self.take(len)
        } else {
            Err(Error::Protocol("RLP: 期望字符串却遇到列表".into()))
        }
    }
    /// 读列表头，返回 payload 长度（并消费头字节）。
    fn list_header(&mut self) -> Result<usize> {
        let b0 = *self.b.get(self.i).ok_or_else(Self::short)?;
        if (0xc0..=0xf7).contains(&b0) {
            self.i += 1;
            Ok((b0 - 0xc0) as usize)
        } else if b0 >= 0xf8 {
            self.i += 1;
            let ll = (b0 - 0xf7) as usize;
            let lb = self.take(ll)?;
            Ok(be_u64(lb) as usize)
        } else {
            Err(Error::Protocol("RLP: 期望列表头".into()))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::airgap::eth as eth_ur;
    use crate::{eth_address, mnemonic_to_seed};

    const TEST_JUNK: &str = "test test test test test test test test test test test junk";
    // 真实 MetaMask/imToken 的 EIP-1559 sign-data（Sepolia，0.01 ETH）。
    const REAL_SIGN_DATA: &str =
        "02f083aa36a7808316735f849ae84ffc825208946e6ebd1f18c3e6c1c2e2ef45dc83ec3724aa912f872386f26fc1000080c0";
    // 上面 sign-data 外层完整的 eth-sign-request payload。
    const REAL_REQUEST: &str = "a701d8255824327271317a337a6964693177367a6c69666d616b7878786a783536617a74666b3561383802583202f083aa36a7808316735f849ae84ffc825208946e6ebd1f18c3e6c1c2e2ef45dc83ec3724aa912f872386f26fc1000080c00304041a00aa36a705d90130a2018a182cf5183cf500f500f400f4021a2ed198a40654c6a234467b725b65dc7f598d853e6be2d3e1ffa00767696d546f6b656e";

    #[test]
    fn decode_eip1559_real_metamask() {
        let sd = hex::decode(REAL_SIGN_DATA).unwrap();
        let s = decode_eip1559(&sd).unwrap();
        assert_eq!(s.chain_id, 11_155_111);
        assert_eq!(s.nonce, 0);
        assert_eq!(s.gas_limit, 21_000);
        assert_eq!(s.value_wei, 10_000_000_000_000_000); // 0.01 ETH
        assert_eq!(s.max_priority_fee_per_gas, 0x0016_735f);
        assert_eq!(s.max_fee_per_gas, 0x9ae8_4ffc);
        assert_eq!(s.data_len, 0);
        assert_eq!(
            hex::encode(s.to.unwrap()),
            "6e6ebd1f18c3e6c1c2e2ef45dc83ec3724aa912f"
        );
    }

    #[test]
    fn sign_recovers_to_derived_address() {
        let seed = mnemonic_to_seed(TEST_JUNK, "").unwrap();
        let sd = hex::decode(REAL_SIGN_DATA).unwrap();
        let sig = sign_sign_data(&seed, 0, 0, &sd).unwrap();
        assert!(sig[64] <= 1, "v 应为 0/1");

        // 从签名+哈希恢复地址，应等于派生地址。
        let hash = keccak256(&sd);
        let signature = Signature::from_slice(&sig[..64]).unwrap();
        let recid = RecoveryId::from_byte(sig[64]).unwrap();
        let vk = k256::ecdsa::VerifyingKey::recover_from_prehash(&hash, &signature, recid).unwrap();
        let unc = vk.to_encoded_point(false);
        let addr20 = &keccak256(&unc.as_bytes()[1..])[12..];
        let expected = eth_address(&seed, 0, 0).unwrap().to_lowercase();
        assert_eq!(format!("0x{}", hex::encode(addr20)), expected);
    }

    // 交叉验证：固件派生的 ETH 私钥字节 == x86 版 rust-bitcoin 派生的字节。
    #[test]
    fn eth_privkey_matches_x86_core() {
        use bitcoin::Network;
        let seed = mnemonic_to_seed(TEST_JUNK, "").unwrap();
        let fw = derive::eth_privkey(&seed, 0, 0).unwrap();
        let wallet =
            btc_wallate_core::seed::Wallet::from_mnemonic(TEST_JUNK, "", Network::Bitcoin).unwrap();
        let x86 = btc_wallate_core::derive::eth_secret_bytes(&wallet, 0, 0).unwrap();
        assert_eq!(fw, x86, "固件与 x86 版派生的 ETH 私钥应逐字节一致");
    }

    #[test]
    fn end_to_end_request_decode_summarize_sign() {
        let seed = mnemonic_to_seed(TEST_JUNK, "").unwrap();
        let payload = hex::decode(REAL_REQUEST).unwrap();
        let req = eth_ur::decode_sign_request(&payload).unwrap();
        assert_eq!(req.data_type, eth_ur::DataType::TypedTransaction);

        // 摘要
        let s = summarize(&req).unwrap();
        assert_eq!(s.chain_id, 11_155_111);
        assert_eq!(s.value_wei, 10_000_000_000_000_000);

        // 签名并可解回 eth-signature
        let (rid, sig) = sign_request(&seed, &req).unwrap();
        let cbor = eth_ur::encode_signature(&rid, &sig).unwrap();
        let (rid2, sig2) = eth_ur::decode_signature(&cbor).unwrap();
        assert_eq!(rid2, req.request_id);
        assert_eq!(sig2, sig);
    }
}
