//! ERC-4527 的 `eth-sign-request` / `eth-signature`，以及 BCR-2020-007 `crypto-keypath`。
//!
//! 以太坊生态没有现成的 Rust UR registry 库，本模块按 [ERC-4527] 的 CBOR 结构手写，
//! 字段键位对齐 Keystone `ur-registry-eth`（TypeScript 权威实现），以保证与 MetaMask
//! （连接 Keystone 空气隙）/ OneKey 等现成观察钱包互操作。
//!
//! 观察钱包 → 签名机：`eth-sign-request`（含待签交易字节 + 派生路径）。
//! 签名机 → 观察钱包：`eth-signature`（65 字节 r‖s‖v）。
//!
//! [ERC-4527]: https://eips.ethereum.org/EIPS/eip-4527

use minicbor::data::Tag;

use crate::{Error, Result};

/// `eth-sign-request` 的 UR 类型串。
pub const SIGN_REQUEST_TYPE: &str = "eth-sign-request";
/// `eth-signature` 的 UR 类型串。
pub const SIGNATURE_TYPE: &str = "eth-signature";

const TAG_UUID: u64 = 37; // request-id 的 CBOR 标签（RFC 8949 UUID）
const TAG_KEYPATH: u64 = 304; // crypto-keypath 的 CBOR 标签（Keystone 旧标签，MetaMask 用）
const TAG_HDKEY: u64 = 303; // crypto-hdkey（仅在嵌套于 multi-accounts 数组时作为标签）

fn enc_err<E: core::fmt::Display>(e: E) -> Error {
    Error::Cbor(e.to_string())
}
fn dec_err(e: minicbor::decode::Error) -> Error {
    Error::Cbor(e.to_string())
}

/// ERC-4527 data-type 枚举（值按 Keystone `ur-registry-eth` 定义，勿改数值映射）：
/// 1=transaction(legacy)，2=typed-data(EIP-712)，3=personal-message，4=typed-transaction(EIP-2718/1559)。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DataType {
    /// 1：legacy（EIP-155）交易的 RLP。
    Transaction,
    /// 2：EIP-712 typed data。
    TypedData,
    /// 3：personal_sign 消息。
    PersonalMessage,
    /// 4：EIP-2718 typed transaction（含 EIP-1559）的字节。
    TypedTransaction,
}

impl DataType {
    fn to_u8(self) -> u8 {
        match self {
            DataType::Transaction => 1,
            DataType::TypedData => 2,
            DataType::PersonalMessage => 3,
            DataType::TypedTransaction => 4,
        }
    }
    fn from_u8(v: u8) -> Result<Self> {
        Ok(match v {
            1 => DataType::Transaction,
            2 => DataType::TypedData,
            3 => DataType::PersonalMessage,
            4 => DataType::TypedTransaction,
            other => return Err(Error::Protocol(format!("未知 eth data-type: {other}"))),
        })
    }
}

/// BIP32 派生路径（crypto-keypath 的常用子集：每级为 (子索引, 是否硬化)）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyPath {
    pub components: Vec<(u32, bool)>,
    /// 主私钥指纹（source-fingerprint）。
    pub source_fingerprint: Option<u32>,
}

impl KeyPath {
    /// 以 `m/44'/60'/0'/0/index` 形式构造一个标准 ETH 账户路径。
    pub fn eth_default(account: u32, index: u32, fingerprint: Option<u32>) -> Self {
        KeyPath {
            components: vec![
                (44, true),
                (60, true),
                (account, true),
                (0, false),
                (index, false),
            ],
            source_fingerprint: fingerprint,
        }
    }

    /// 若为标准 ETH 路径 `m/44'/60'/account'/0/index`，返回 `(account, index)`。
    /// 用于从签名请求里安全地定位要用哪把派生私钥（拒绝非常规路径，防误签）。
    pub fn eth_account_index(&self) -> Option<(u32, u32)> {
        let c = &self.components;
        if c.len() == 5
            && c[0] == (44, true)
            && c[1] == (60, true)
            && c[2].1 // account 硬化
            && c[3] == (0, false)
            && !c[4].1 // index 非硬化
        {
            Some((c[2].0, c[4].0))
        } else {
            None
        }
    }
}

/// 解码出的以太坊签名请求。
#[derive(Debug, Clone)]
pub struct EthSignRequest {
    /// 请求 id（通常 16 字节 UUID）。签回的 eth-signature 必须原样带回。
    pub request_id: Vec<u8>,
    /// 待签数据：随 `data_type` 而定，交易类为交易字节。
    pub sign_data: Vec<u8>,
    pub data_type: DataType,
    pub chain_id: Option<u64>,
    pub derivation: KeyPath,
    /// 预期签名地址（20 字节，可选，用于签名机侧再次核对）。
    pub address: Option<Vec<u8>>,
}

// ---------- crypto-keypath ----------

fn encode_keypath<W: minicbor::encode::Write>(
    e: &mut minicbor::Encoder<W>,
    kp: &KeyPath,
) -> Result<()>
where
    W::Error: core::fmt::Display,
{
    e.tag(Tag::new(TAG_KEYPATH)).map_err(enc_err)?;
    let fields = 1 + u64::from(kp.source_fingerprint.is_some());
    e.map(fields).map_err(enc_err)?;
    // key 1: components，扁平数组 [idx0, hard0, idx1, hard1, ...]
    e.u8(1).map_err(enc_err)?;
    e.array((kp.components.len() * 2) as u64).map_err(enc_err)?;
    for (idx, hardened) in &kp.components {
        e.u32(*idx).map_err(enc_err)?;
        e.bool(*hardened).map_err(enc_err)?;
    }
    // key 2: source-fingerprint
    if let Some(fp) = kp.source_fingerprint {
        e.u8(2).map_err(enc_err)?;
        e.u32(fp).map_err(enc_err)?;
    }
    Ok(())
}

fn decode_keypath(d: &mut minicbor::Decoder) -> Result<KeyPath> {
    let tag = d.tag().map_err(dec_err)?;
    if tag != Tag::new(TAG_KEYPATH) {
        return Err(Error::Protocol(format!(
            "crypto-keypath 期望标签 304，实际 {tag:?}"
        )));
    }
    let n = d
        .map()
        .map_err(dec_err)?
        .ok_or_else(|| Error::Protocol("crypto-keypath 需为定长 map".into()))?;

    let mut components = Vec::new();
    let mut source_fingerprint = None;
    for _ in 0..n {
        let key = d.u32().map_err(dec_err)?;
        match key {
            1 => {
                let len = d
                    .array()
                    .map_err(dec_err)?
                    .ok_or_else(|| Error::Protocol("keypath components 需为定长数组".into()))?;
                if len % 2 != 0 {
                    return Err(Error::Protocol("keypath components 元素数应为偶数".into()));
                }
                let mut i = 0;
                while i < len {
                    // 仅支持简单子索引（uint）+ 硬化布尔；不支持范围/通配。
                    let idx = d.u32().map_err(dec_err)?;
                    let hardened = d.bool().map_err(dec_err)?;
                    components.push((idx, hardened));
                    i += 2;
                }
            }
            2 => source_fingerprint = Some(d.u32().map_err(dec_err)?),
            _ => d.skip().map_err(dec_err)?, // depth 等其它字段忽略
        }
    }
    Ok(KeyPath {
        components,
        source_fingerprint,
    })
}

// ---------- eth-sign-request ----------

/// 编码 `eth-sign-request` 的 CBOR payload。
pub fn encode_sign_request(req: &EthSignRequest) -> Result<Vec<u8>> {
    let mut e = minicbor::Encoder::new(Vec::new());
    let fields = 4
        + u64::from(req.chain_id.is_some())
        + u64::from(req.address.is_some());
    e.map(fields).map_err(enc_err)?;

    // 1: request-id (#6.37 bstr)
    e.u8(1).map_err(enc_err)?;
    e.tag(Tag::new(TAG_UUID)).map_err(enc_err)?;
    e.bytes(&req.request_id).map_err(enc_err)?;
    // 2: sign-data
    e.u8(2).map_err(enc_err)?;
    e.bytes(&req.sign_data).map_err(enc_err)?;
    // 3: data-type
    e.u8(3).map_err(enc_err)?;
    e.u8(req.data_type.to_u8()).map_err(enc_err)?;
    // 4: chain-id（可选）
    if let Some(cid) = req.chain_id {
        e.u8(4).map_err(enc_err)?;
        e.u64(cid).map_err(enc_err)?;
    }
    // 5: derivation-path (#6.304 crypto-keypath)
    e.u8(5).map_err(enc_err)?;
    encode_keypath(&mut e, &req.derivation)?;
    // 6: address（可选）
    if let Some(addr) = &req.address {
        e.u8(6).map_err(enc_err)?;
        e.bytes(addr).map_err(enc_err)?;
    }
    Ok(e.into_writer())
}

/// 解码 `eth-sign-request` 的 CBOR payload。
pub fn decode_sign_request(cbor: &[u8]) -> Result<EthSignRequest> {
    let mut d = minicbor::Decoder::new(cbor);
    let n = d
        .map()
        .map_err(dec_err)?
        .ok_or_else(|| Error::Protocol("eth-sign-request 需为定长 map".into()))?;

    let mut request_id = None;
    let mut sign_data = None;
    let mut data_type = None;
    let mut chain_id = None;
    let mut derivation = None;
    let mut address = None;

    for _ in 0..n {
        let key = d.u32().map_err(dec_err)?;
        match key {
            1 => {
                let tag = d.tag().map_err(dec_err)?;
                if tag != Tag::new(TAG_UUID) {
                    return Err(Error::Protocol(format!(
                        "request-id 期望标签 37，实际 {tag:?}"
                    )));
                }
                request_id = Some(d.bytes().map_err(dec_err)?.to_vec());
            }
            2 => sign_data = Some(d.bytes().map_err(dec_err)?.to_vec()),
            3 => data_type = Some(DataType::from_u8(d.u8().map_err(dec_err)?)?),
            4 => chain_id = Some(d.u64().map_err(dec_err)?),
            5 => derivation = Some(decode_keypath(&mut d)?),
            6 => address = Some(d.bytes().map_err(dec_err)?.to_vec()),
            _ => d.skip().map_err(dec_err)?, // origin 等忽略
        }
    }

    Ok(EthSignRequest {
        request_id: request_id
            .ok_or_else(|| Error::Protocol("eth-sign-request 缺 request-id".into()))?,
        sign_data: sign_data.ok_or_else(|| Error::Protocol("eth-sign-request 缺 sign-data".into()))?,
        data_type: data_type
            .ok_or_else(|| Error::Protocol("eth-sign-request 缺 data-type".into()))?,
        chain_id,
        derivation: derivation
            .ok_or_else(|| Error::Protocol("eth-sign-request 缺 derivation-path".into()))?,
        address,
    })
}

// ---------- eth-signature ----------

/// 编码 `eth-signature` 的 CBOR payload。`signature` 为 65 字节 r‖s‖v。
pub fn encode_signature(request_id: &[u8], signature: &[u8; 65]) -> Result<Vec<u8>> {
    let mut e = minicbor::Encoder::new(Vec::new());
    e.map(2).map_err(enc_err)?;
    e.u8(1).map_err(enc_err)?;
    e.tag(Tag::new(TAG_UUID)).map_err(enc_err)?;
    e.bytes(request_id).map_err(enc_err)?;
    e.u8(2).map_err(enc_err)?;
    e.bytes(signature).map_err(enc_err)?;
    Ok(e.into_writer())
}

/// 解码 `eth-signature`，返回 (request_id, 65 字节签名)。
pub fn decode_signature(cbor: &[u8]) -> Result<(Vec<u8>, [u8; 65])> {
    let mut d = minicbor::Decoder::new(cbor);
    let n = d
        .map()
        .map_err(dec_err)?
        .ok_or_else(|| Error::Protocol("eth-signature 需为定长 map".into()))?;
    let mut request_id = None;
    let mut sig = None;
    for _ in 0..n {
        let key = d.u32().map_err(dec_err)?;
        match key {
            1 => {
                let tag = d.tag().map_err(dec_err)?;
                if tag != Tag::new(TAG_UUID) {
                    return Err(Error::Protocol("eth-signature request-id 标签应为 37".into()));
                }
                request_id = Some(d.bytes().map_err(dec_err)?.to_vec());
            }
            2 => {
                let b = d.bytes().map_err(dec_err)?;
                let arr: [u8; 65] = b
                    .try_into()
                    .map_err(|_| Error::Protocol("eth-signature 签名应为 65 字节".into()))?;
                sig = Some(arr);
            }
            _ => d.skip().map_err(dec_err)?,
        }
    }
    Ok((
        request_id.ok_or_else(|| Error::Protocol("eth-signature 缺 request-id".into()))?,
        sig.ok_or_else(|| Error::Protocol("eth-signature 缺 signature".into()))?,
    ))
}

/// 便捷：签名请求 → 动画二维码分帧。
pub fn sign_request_to_ur_parts(
    req: &EthSignRequest,
    max_fragment_length: usize,
    parts: usize,
) -> Result<Vec<String>> {
    super::encode_parts(SIGN_REQUEST_TYPE, &encode_sign_request(req)?, max_fragment_length, parts)
}

/// 便捷：签名 → 单帧二维码（65 字节签名很短，一帧足够）。
pub fn signature_to_ur_single(request_id: &[u8], signature: &[u8; 65]) -> Result<String> {
    Ok(super::encode_single(SIGNATURE_TYPE, &encode_signature(request_id, signature)?))
}

// ---------- crypto-multi-accounts（账户配对导出，供 MetaMask 连接） ----------

/// `crypto-multi-accounts` 的 UR 类型串。
pub const MULTI_ACCOUNTS_TYPE: &str = "crypto-multi-accounts";

/// 一个账户级扩展公钥（crypto-hdkey），供观察端派生地址。
///
/// 对 ETH：`key_data` 为账户节点 `m/44'/60'/account'` 的 33 字节压缩公钥，`chain_code` 为其链码，
/// 观察端据此派生 `.../0/x` 地址；`components` 为该节点的派生路径。
#[derive(Debug, Clone)]
pub struct AccountKey {
    pub key_data: [u8; 33],
    pub chain_code: [u8; 32],
    pub components: Vec<(u32, bool)>,
    pub source_fingerprint: u32,
    pub parent_fingerprint: u32,
    pub name: String,
}

/// crypto-hdkey 的 UR 类型串。
pub const HDKEY_TYPE: &str = "crypto-hdkey";

/// 写 crypto-hdkey 的裸 map（不含顶层标签）。字段与 MetaMask/imToken 例子一致：
/// key-data(3) + chain-code(4) + origin(6, keypath tag304) + parent-fingerprint(8) + name(9)。
fn write_hdkey_map(e: &mut minicbor::Encoder<Vec<u8>>, key: &AccountKey) -> Result<()> {
    e.map(5).map_err(enc_err)?;
    e.u8(3).map_err(enc_err)?;
    e.bytes(&key.key_data).map_err(enc_err)?;
    e.u8(4).map_err(enc_err)?;
    e.bytes(&key.chain_code).map_err(enc_err)?;
    e.u8(6).map_err(enc_err)?;
    encode_keypath(
        e,
        &KeyPath {
            components: key.components.clone(),
            source_fingerprint: Some(key.source_fingerprint),
        },
    )?;
    e.u8(8).map_err(enc_err)?;
    e.u32(key.parent_fingerprint).map_err(enc_err)?;
    e.u8(9).map_err(enc_err)?;
    e.str(&key.name).map_err(enc_err)?;
    Ok(())
}

/// 编码独立的 `crypto-hdkey`（裸 map，无顶层标签），MetaMask「连接硬件钱包 → QR」用此配对。
pub fn encode_hdkey(key: &AccountKey) -> Result<Vec<u8>> {
    let mut e = minicbor::Encoder::new(Vec::new());
    write_hdkey_map(&mut e, key)?;
    Ok(e.into_writer())
}

/// 便捷：crypto-hdkey → 单条 UR 字符串。
pub fn hdkey_to_ur_single(key: &AccountKey) -> Result<String> {
    Ok(super::encode_single(HDKEY_TYPE, &encode_hdkey(key)?))
}

/// 嵌套用：带 tag 303 的 crypto-hdkey（用于 crypto-multi-accounts 数组内）。
fn encode_hdkey_nested(e: &mut minicbor::Encoder<Vec<u8>>, key: &AccountKey) -> Result<()> {
    e.tag(Tag::new(TAG_HDKEY)).map_err(enc_err)?;
    write_hdkey_map(e, key)
}

/// 编码 `crypto-multi-accounts`（含若干账户 hdkey），供 MetaMask 等扫码配对。
pub fn encode_multi_accounts(
    master_fingerprint: u32,
    keys: &[AccountKey],
    device: &str,
) -> Result<Vec<u8>> {
    // 顶层为裸 map：UR 类型串已标明是 crypto-multi-accounts，故顶层不加 tag 1103
    //（标签只用于嵌套对象：数组内的 hdkey 带 tag 303、其 origin keypath 带 tag 304）。
    let mut e = minicbor::Encoder::new(Vec::new());
    let fields = 2 + u64::from(!device.is_empty());
    e.map(fields).map_err(enc_err)?;
    e.u8(1).map_err(enc_err)?; // master-fingerprint
    e.u32(master_fingerprint).map_err(enc_err)?;
    e.u8(2).map_err(enc_err)?; // keys
    e.array(keys.len() as u64).map_err(enc_err)?;
    for k in keys {
        encode_hdkey_nested(&mut e, k)?;
    }
    if !device.is_empty() {
        e.u8(3).map_err(enc_err)?; // device
        e.str(device).map_err(enc_err)?;
    }
    Ok(e.into_writer())
}

/// 便捷：账户导出 → 单帧二维码（账户数据很短，一帧足够）。
pub fn multi_accounts_to_ur_single(
    master_fingerprint: u32,
    keys: &[AccountKey],
    device: &str,
) -> Result<String> {
    Ok(super::encode_single(
        MULTI_ACCOUNTS_TYPE,
        &encode_multi_accounts(master_fingerprint, keys, device)?,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::airgap::PartCollector;

    fn sample_request() -> EthSignRequest {
        EthSignRequest {
            request_id: vec![
                0x9b, 0x1d, 0xeb, 0x4d, 0x3b, 0x7d, 0x4b, 0xad, 0x9b, 0xdd, 0x2b, 0x0d, 0x7b, 0x3d,
                0xcb, 0x6d,
            ],
            sign_data: vec![0x02, 0xde, 0xad, 0xbe, 0xef], // 假装的 EIP-2718 typed tx 字节
            data_type: DataType::TypedTransaction,
            chain_id: Some(11_155_111),
            derivation: KeyPath::eth_default(0, 0, Some(0x1250_b6bc)),
            address: Some(vec![0x11; 20]),
        }
    }

    #[test]
    fn keypath_default_is_bip44_eth() {
        let kp = KeyPath::eth_default(0, 5, None);
        assert_eq!(
            kp.components,
            vec![(44, true), (60, true), (0, true), (0, false), (5, false)]
        );
    }

    #[test]
    fn sign_request_cbor_roundtrip() {
        let req = sample_request();
        let cbor = encode_sign_request(&req).unwrap();
        let back = decode_sign_request(&cbor).unwrap();
        assert_eq!(back.request_id, req.request_id);
        assert_eq!(back.sign_data, req.sign_data);
        assert_eq!(back.data_type, req.data_type);
        assert_eq!(back.chain_id, req.chain_id);
        assert_eq!(back.derivation, req.derivation);
        assert_eq!(back.address, req.address);
    }

    #[test]
    fn sign_request_minimal_without_optionals() {
        let req = EthSignRequest {
            request_id: vec![1, 2, 3, 4],
            sign_data: vec![0xaa, 0xbb],
            data_type: DataType::Transaction,
            chain_id: None,
            derivation: KeyPath::eth_default(0, 0, None),
            address: None,
        };
        let cbor = encode_sign_request(&req).unwrap();
        let back = decode_sign_request(&cbor).unwrap();
        assert_eq!(back.chain_id, None);
        assert_eq!(back.address, None);
        assert_eq!(back.data_type, DataType::Transaction);
    }

    #[test]
    fn signature_cbor_roundtrip() {
        let rid = vec![0xab; 16];
        let mut sig = [0u8; 65];
        for (i, b) in sig.iter_mut().enumerate() {
            *b = i as u8;
        }
        let cbor = encode_signature(&rid, &sig).unwrap();
        let (rid2, sig2) = decode_signature(&cbor).unwrap();
        assert_eq!(rid2, rid);
        assert_eq!(sig2, sig);
    }

    // 锁死线格式（RFC 8949 CBOR）以保证与外部钱包互操作，而非仅自洽 round-trip：
    //   map(2)=0xA2, key1=0x01, tag37=0xD8 0x25, bytes(16)=0x50, key2=0x02, bytes(65)=0x58 0x41
    #[test]
    fn signature_cbor_matches_spec_byte_layout() {
        let rid = vec![0xab; 16];
        let sig = [0u8; 65];
        let cbor = encode_signature(&rid, &sig).unwrap();
        assert_eq!(&cbor[0..3], &[0xA2, 0x01, 0xD8]);
        assert_eq!(cbor[3], 0x25); // tag 37 的第二字节
        assert_eq!(cbor[4], 0x50); // bytes, 长度 16
        assert_eq!(cbor[4 + 1 + 16], 0x02); // key 2
        assert_eq!(&cbor[4 + 1 + 16 + 1..4 + 1 + 16 + 3], &[0x58, 0x41]); // bytes, 长度 65
    }

    // eth-sign-request 起始字节：map(6)=0xA6, key1=0x01, tag37=0xD8 0x25, request-id bytes(16)=0x50
    #[test]
    fn sign_request_cbor_header_matches_spec() {
        let req = sample_request(); // 含 chain_id + address ⇒ 6 个字段
        let cbor = encode_sign_request(&req).unwrap();
        assert_eq!(&cbor[0..5], &[0xA6, 0x01, 0xD8, 0x25, 0x50]);
    }

    // 黄金测试：用 MetaMask 文档中 imToken 配对样例的字段，逐字节复现其 crypto-hdkey CBOR。
    // 样例 UR: UR:CRYPTO-HDKEY/ONAXHDCL...（见 MetaMask 硬件钱包文档）。
    #[test]
    fn hdkey_matches_metamask_imtoken_example() {
        let key_data: [u8; 33] = hex_arr(
            "031f0726617444b6ec04b96a48b8a2b7aff8883dc76966995c0b0f8c130fdc8aa2",
        );
        let chain_code: [u8; 32] =
            hex_arr("b2854771ef82921e9eb9b97b6e1822b0f72aff18b1648a77be580a7a052c50b8");
        let key = AccountKey {
            key_data,
            chain_code,
            components: vec![(44, true), (60, true), (0, true)],
            source_fingerprint: 0xc2f1_de41,
            parent_fingerprint: 0xc2f1_de41,
            name: "imToken-Default 1".into(),
        };
        let cbor = encode_hdkey(&key).unwrap();
        let expected = "a5035821031f0726617444b6ec04b96a48b8a2b7aff8883dc76966995c0b0f8c130fdc8aa2045820b2854771ef82921e9eb9b97b6e1822b0f72aff18b1648a77be580a7a052c50b806d90130a20186182cf5183cf500f5021ac2f1de41081ac2f1de410971696d546f6b656e2d44656661756c742031";
        assert_eq!(hex::encode(&cbor), expected, "应逐字节复现 MetaMask 样例");
    }

    fn hex_arr<const N: usize>(s: &str) -> [u8; N] {
        let v = hex::decode(s).unwrap();
        let mut a = [0u8; N];
        a.copy_from_slice(&v);
        a
    }

    // 回归：真实 MetaMask/imToken 的 eth-sign-request（Sepolia EIP-1559）。
    // data-type=4 应解为 TypedTransaction，sign-data 为 0x02 前缀的 EIP-1559 交易。
    #[test]
    fn decode_real_metamask_sign_request() {
        let payload = hex::decode(
            "a701d8255824327271317a337a6964693177367a6c69666d616b7878786a783536617a74666b3561383802583202f083aa36a7808316735f849ae84ffc825208946e6ebd1f18c3e6c1c2e2ef45dc83ec3724aa912f872386f26fc1000080c00304041a00aa36a705d90130a2018a182cf5183cf500f500f400f4021a2ed198a40654c6a234467b725b65dc7f598d853e6be2d3e1ffa00767696d546f6b656e",
        )
        .unwrap();
        let req = decode_sign_request(&payload).unwrap();
        assert_eq!(req.data_type, DataType::TypedTransaction);
        assert_eq!(req.chain_id, Some(11_155_111));
        assert_eq!(req.sign_data.first(), Some(&0x02u8)); // EIP-1559 类型前缀
        assert_eq!(req.derivation.eth_account_index(), Some((0, 0)));
    }

    #[test]
    fn multi_accounts_wire_format() {
        let key = AccountKey {
            key_data: [0x02; 33],
            chain_code: [0x11; 32],
            components: vec![(44, true), (60, true), (0, true)],
            source_fingerprint: 0x1250_b6bc,
            parent_fingerprint: 0xdead_beef,
            name: "ETH #0".into(),
        };
        let cbor = encode_multi_accounts(0x1250_b6bc, std::slice::from_ref(&key), "btc-wallate").unwrap();
        // 顶层为裸 map（无 tag 1103）：map(3)=0xA3, key1=0x01, master-fp uint32=0x1A + 4 字节
        assert_eq!(&cbor[0..3], &[0xA3, 0x01, 0x1A]);
        assert_eq!(cbor[7], 0x02, "key 2 = keys");
        assert_eq!(cbor[8], 0x81, "keys 为含 1 个元素的数组");
        // 嵌套 crypto-hdkey 带 tag 303 = 0xD9 0x01 0x2F
        assert_eq!(&cbor[9..12], &[0xD9, 0x01, 0x2F]);
        // 嵌套 crypto-keypath 带 tag 304 = 0xD9 0x01 0x30
        assert!(cbor.windows(3).any(|w| w == [0xD9, 0x01, 0x30]));
    }

    #[test]
    fn full_ur_qr_roundtrip_sign_request() {
        let req = sample_request();
        let parts = sign_request_to_ur_parts(&req, 40, 30).unwrap();
        let mut c = PartCollector::new();
        for p in &parts {
            if c.is_complete() {
                break;
            }
            c.receive(p).unwrap();
        }
        assert!(c.is_complete());
        assert_eq!(c.ur_type(), Some(SIGN_REQUEST_TYPE));
        let payload = c.payload().unwrap().unwrap();
        let back = decode_sign_request(&payload).unwrap();
        assert_eq!(back.derivation, req.derivation);
        assert_eq!(back.sign_data, req.sign_data);
    }
}
