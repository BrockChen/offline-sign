//! BTC 签名（P2WPKH，BIP-143 sighash + k256），纯 Rust、不依赖 rust-bitcoin。
//!
//! 最小 PSBT(BIP-174) 解析：把每段解析为「原始 key-value 列表」以忠实保留所有字段；
//! 对本钱包拥有的输入（bip32_derivation 里派生出的公钥与我方一致）计算 BIP-143 sighash、
//! 用 k256 出 DER 签名、回填 partial_sig(0x02)，再原样重组 PSBT。仅支持 SIGHASH_ALL 的 P2WPKH。

use k256::ecdsa::SigningKey;
use ripemd::Ripemd160;
use sha2::{Digest, Sha256};

use crate::derive::Net;
use crate::{derive, Error, Result};

const PSBT_MAGIC: [u8; 5] = [0x70, 0x73, 0x62, 0x74, 0xff];

type Kv = (Vec<u8>, Vec<u8>);

fn dsha256(data: &[u8]) -> [u8; 32] {
    let mut o = [0u8; 32];
    o.copy_from_slice(&Sha256::digest(Sha256::digest(data)));
    o
}
fn hash160(data: &[u8]) -> [u8; 20] {
    let mut o = [0u8; 20];
    o.copy_from_slice(&Ripemd160::digest(Sha256::digest(data)));
    o
}

struct Rd<'a> {
    b: &'a [u8],
    i: usize,
}
impl<'a> Rd<'a> {
    fn new(b: &'a [u8]) -> Self {
        Rd { b, i: 0 }
    }
    fn short() -> Error {
        Error::Protocol("PSBT 数据过短".into())
    }
    fn take(&mut self, n: usize) -> Result<&'a [u8]> {
        let end = self.i.checked_add(n).ok_or_else(Self::short)?;
        let s = self.b.get(self.i..end).ok_or_else(Self::short)?;
        self.i = end;
        Ok(s)
    }
    fn u8(&mut self) -> Result<u8> {
        Ok(self.take(1)?[0])
    }
    fn cs(&mut self) -> Result<u64> {
        let n = self.u8()?;
        Ok(match n {
            0xfd => {
                let b = self.take(2)?;
                u16::from_le_bytes([b[0], b[1]]) as u64
            }
            0xfe => {
                let b = self.take(4)?;
                u32::from_le_bytes([b[0], b[1], b[2], b[3]]) as u64
            }
            0xff => {
                let b = self.take(8)?;
                u64::from_le_bytes(b.try_into().unwrap())
            }
            x => x as u64,
        })
    }
    fn eof(&self) -> bool {
        self.i >= self.b.len()
    }
}

fn parse_map(rd: &mut Rd) -> Result<Vec<Kv>> {
    let mut kvs = Vec::new();
    loop {
        let klen = rd.cs()?;
        if klen == 0 {
            break;
        }
        let key = rd.take(klen as usize)?.to_vec();
        let vlen = rd.cs()?;
        let val = rd.take(vlen as usize)?.to_vec();
        kvs.push((key, val));
    }
    Ok(kvs)
}

fn write_cs(out: &mut Vec<u8>, n: u64) {
    if n < 0xfd {
        out.push(n as u8);
    } else if n <= 0xffff {
        out.push(0xfd);
        out.extend_from_slice(&(n as u16).to_le_bytes());
    } else if n <= 0xffff_ffff {
        out.push(0xfe);
        out.extend_from_slice(&(n as u32).to_le_bytes());
    } else {
        out.push(0xff);
        out.extend_from_slice(&n.to_le_bytes());
    }
}

fn emit_map(out: &mut Vec<u8>, kvs: &[Kv]) {
    for (k, v) in kvs {
        write_cs(out, k.len() as u64);
        out.extend_from_slice(k);
        write_cs(out, v.len() as u64);
        out.extend_from_slice(v);
    }
    out.push(0x00);
}

/// 从未签名交易里抽取 BIP-143 需要的部分。
struct TxParts {
    version: [u8; 4],
    locktime: [u8; 4],
    prevouts: Vec<u8>,  // 各输入 outpoint(36) 拼接
    sequences: Vec<u8>, // 各输入 sequence(4) 拼接
    outputs: Vec<u8>,   // 各输出原始序列化拼接
    n_in: usize,
    n_out: usize,
}

fn parse_tx(b: &[u8]) -> Result<TxParts> {
    let mut rd = Rd::new(b);
    let version: [u8; 4] = rd.take(4)?.try_into().unwrap();
    let n_in = rd.cs()? as usize;
    let mut prevouts = Vec::new();
    let mut sequences = Vec::new();
    for _ in 0..n_in {
        prevouts.extend_from_slice(rd.take(36)?); // txid(32)+vout(4)
        let sl = rd.cs()? as usize;
        rd.take(sl)?; // scriptSig（未签名为空）
        sequences.extend_from_slice(rd.take(4)?);
    }
    let n_out = rd.cs()? as usize;
    let mut outputs = Vec::new();
    for _ in 0..n_out {
        let start = rd.i;
        rd.take(8)?; // value
        let sl = rd.cs()? as usize;
        rd.take(sl)?;
        outputs.extend_from_slice(&b[start..rd.i]);
    }
    let locktime: [u8; 4] = rd.take(4)?.try_into().unwrap();
    Ok(TxParts {
        version,
        locktime,
        prevouts,
        sequences,
        outputs,
        n_in,
        n_out,
    })
}

/// 由 bip32_derivation 的 value（4 字节指纹 + 若干 4 字节 LE 子号）重建 `m/...` 路径。
fn path_from_deriv(val: &[u8]) -> String {
    let mut s = String::from("m");
    let mut i = 4;
    while i + 4 <= val.len() {
        let cn = u32::from_le_bytes([val[i], val[i + 1], val[i + 2], val[i + 3]]);
        let hardened = cn & 0x8000_0000 != 0;
        let idx = cn & 0x7fff_ffff;
        s.push('/');
        s.push_str(&idx.to_string());
        if hardened {
            s.push('\'');
        }
        i += 4;
    }
    s
}

fn bip143_sighash(
    tx: &TxParts,
    idx: usize,
    hash_prevouts: &[u8; 32],
    hash_sequence: &[u8; 32],
    hash_outputs: &[u8; 32],
    pubkey33: &[u8],
    amount_le8: &[u8],
) -> [u8; 32] {
    let outpoint = &tx.prevouts[idx * 36..idx * 36 + 36];
    let sequence = &tx.sequences[idx * 4..idx * 4 + 4];
    let pkh = hash160(pubkey33);
    let mut pre = Vec::with_capacity(156);
    pre.extend_from_slice(&tx.version);
    pre.extend_from_slice(hash_prevouts);
    pre.extend_from_slice(hash_sequence);
    pre.extend_from_slice(outpoint);
    // P2WPKH scriptCode: 0x19 76 a9 14 <20B> 88 ac
    pre.push(0x19);
    pre.extend_from_slice(&[0x76, 0xa9, 0x14]);
    pre.extend_from_slice(&pkh);
    pre.extend_from_slice(&[0x88, 0xac]);
    pre.extend_from_slice(amount_le8);
    pre.extend_from_slice(sequence);
    pre.extend_from_slice(hash_outputs);
    pre.extend_from_slice(&tx.locktime);
    pre.extend_from_slice(&[0x01, 0x00, 0x00, 0x00]); // SIGHASH_ALL
    dsha256(&pre)
}

/// 对 PSBT 里本钱包拥有的 P2WPKH 输入签名，返回带 partial_sig 的 PSBT 字节。
pub fn sign_psbt(seed: &[u8], psbt: &[u8]) -> Result<Vec<u8>> {
    let mut rd = Rd::new(psbt);
    if rd.take(5)? != PSBT_MAGIC {
        return Err(Error::Protocol("PSBT magic 不匹配".into()));
    }
    let global = parse_map(&mut rd)?;
    let unsigned = global
        .iter()
        .find(|(k, _)| k.as_slice() == [0x00])
        .ok_or_else(|| Error::Protocol("PSBT 缺 unsigned tx".into()))?
        .1
        .clone();
    let tx = parse_tx(&unsigned)?;

    let mut inputs = Vec::with_capacity(tx.n_in);
    for _ in 0..tx.n_in {
        inputs.push(parse_map(&mut rd)?);
    }
    let mut outputs = Vec::with_capacity(tx.n_out);
    for _ in 0..tx.n_out {
        outputs.push(parse_map(&mut rd)?);
    }
    if !rd.eof() {
        return Err(Error::Protocol("PSBT 尾部有多余字节".into()));
    }

    let hash_prevouts = dsha256(&tx.prevouts);
    let hash_sequence = dsha256(&tx.sequences);
    let hash_outputs = dsha256(&tx.outputs);

    let mut signed = 0usize;
    for (idx, inp) in inputs.iter_mut().enumerate() {
        // witness_utxo (key 0x01) = value(8 LE) + scriptPubKey(compactsize+script)
        let amount: Vec<u8> = match inp.iter().find(|(k, _)| k.as_slice() == [0x01]) {
            Some((_, v)) if v.len() >= 8 => v[0..8].to_vec(),
            _ => continue, // 非 witness 输入，跳过
        };
        // bip32_derivation: key = 0x06 ++ 33B pubkey，value = 4B 指纹 + 路径
        let derivs: Vec<(Vec<u8>, Vec<u8>)> = inp
            .iter()
            .filter(|(k, _)| k.first() == Some(&0x06) && k.len() == 34)
            .map(|(k, v)| (k[1..].to_vec(), v.clone()))
            .collect();
        for (pubkey, val) in derivs {
            let path = path_from_deriv(&val);
            let derived = derive::pubkey_compressed(seed, &path)?;
            if derived.as_slice() != pubkey.as_slice() {
                continue; // 不是本钱包的 key
            }
            let sighash = bip143_sighash(
                &tx,
                idx,
                &hash_prevouts,
                &hash_sequence,
                &hash_outputs,
                &pubkey,
                &amount,
            );
            let sk_bytes = derive::privkey(seed, &path)?;
            let sk = SigningKey::from_slice(&sk_bytes).map_err(|e| Error::Sign(format!("{e}")))?;
            let (sig, _recid) = sk
                .sign_prehash_recoverable(&sighash)
                .map_err(|e| Error::Sign(format!("{e}")))?;
            let mut sigval = sig.to_der().as_bytes().to_vec();
            sigval.push(0x01); // SIGHASH_ALL
            let mut key = Vec::with_capacity(34);
            key.push(0x02); // partial_sig
            key.extend_from_slice(&pubkey);
            inp.push((key, sigval));
            signed += 1;
        }
    }
    if signed == 0 {
        return Err(Error::Protocol("没有可签名的输入（无本钱包拥有的 P2WPKH 输入）".into()));
    }

    let mut out = Vec::with_capacity(psbt.len() + signed * 80);
    out.extend_from_slice(&PSBT_MAGIC);
    emit_map(&mut out, &global);
    for inp in &inputs {
        emit_map(&mut out, inp);
    }
    for o in &outputs {
        emit_map(&mut out, o);
    }
    Ok(out)
}

/// 一笔 PSBT 的屏幕核对摘要。
#[derive(Debug, Clone)]
pub struct PsbtSummary {
    /// 每个输出：(地址或 None, 金额 sat)。
    pub outputs: Vec<(Option<String>, u64)>,
    pub total_out: u64,
    /// 手续费（所有输入 witness_utxo 金额已知时可算）。
    pub fee: Option<u64>,
}

fn spk_to_addr(net: Net, spk: &[u8]) -> Option<String> {
    let hrp = match net {
        Net::Mainnet => "bc",
        Net::Test => "tb",
    };
    let program = if spk.len() == 22 && spk[0] == 0x00 && spk[1] == 0x14 {
        &spk[2..22] // P2WPKH
    } else if spk.len() == 34 && spk[0] == 0x00 && spk[1] == 0x20 {
        &spk[2..34] // P2WSH
    } else {
        return None; // 其它脚本类型（含 taproot）交由上层显示 hex
    };
    bech32::segwit::encode_v0(bech32::Hrp::parse(hrp).ok()?, program).ok()
}

/// 解析 PSBT 产出屏幕核对摘要（输出金额/地址 + 手续费）。
pub fn summarize_psbt(net: Net, psbt: &[u8]) -> Result<PsbtSummary> {
    let mut rd = Rd::new(psbt);
    if rd.take(5)? != PSBT_MAGIC {
        return Err(Error::Protocol("PSBT magic 不匹配".into()));
    }
    let global = parse_map(&mut rd)?;
    let unsigned = global
        .iter()
        .find(|(k, _)| k.as_slice() == [0x00])
        .ok_or_else(|| Error::Protocol("PSBT 缺 unsigned tx".into()))?
        .1
        .clone();
    let tx = parse_tx(&unsigned)?;

    // 输入 witness_utxo 金额之和。
    let mut total_in = 0u64;
    let mut have_all = true;
    for _ in 0..tx.n_in {
        let inp = parse_map(&mut rd)?;
        match inp.iter().find(|(k, _)| k.as_slice() == [0x01]) {
            Some((_, v)) if v.len() >= 8 => {
                total_in += u64::from_le_bytes(v[0..8].try_into().unwrap())
            }
            _ => have_all = false,
        }
    }

    // 输出（金额 + scriptPubKey）来自未签名交易。
    let mut oc = Rd::new(&tx.outputs);
    let mut outputs = Vec::with_capacity(tx.n_out);
    let mut total_out = 0u64;
    for _ in 0..tx.n_out {
        let value = u64::from_le_bytes(oc.take(8)?.try_into().unwrap());
        let sl = oc.cs()? as usize;
        let spk = oc.take(sl)?;
        total_out += value;
        outputs.push((spk_to_addr(net, spk), value));
    }

    let fee = if have_all {
        total_in.checked_sub(total_out)
    } else {
        None
    };
    Ok(PsbtSummary {
        outputs,
        total_out,
        fee,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mnemonic_to_seed;
    use bitcoin::bip32::{ChildNumber, DerivationPath};
    use bitcoin::psbt::{Input, Psbt};
    use bitcoin::secp256k1::Secp256k1;
    use bitcoin::{
        absolute::LockTime, transaction::Version, Address, Amount, Network, OutPoint, Sequence,
        Transaction, TxIn, TxOut, Txid, Witness,
    };
    use btc_wallate_core::seed::Wallet;
    use std::str::FromStr;

    const ABANDON: &str =
        "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";

    // 用 rust-bitcoin 构造一个本钱包 m/84'/1'/0'/0/0 可签的 P2WPKH PSBT（同 x86 core 测试）。
    fn build_signable_psbt(w: &Wallet) -> Psbt {
        let secp = Secp256k1::new();
        let path = DerivationPath::from(vec![
            ChildNumber::from_hardened_idx(84).unwrap(),
            ChildNumber::from_hardened_idx(1).unwrap(),
            ChildNumber::from_hardened_idx(0).unwrap(),
            ChildNumber::from_normal_idx(0).unwrap(),
            ChildNumber::from_normal_idx(0).unwrap(),
        ]);
        let xpriv = w.master_xpriv().derive_priv(&secp, &path).unwrap();
        let pk = bitcoin::PublicKey::new(xpriv.private_key.public_key(&secp));
        let fingerprint = w.master_xpriv().fingerprint(&secp);
        let my_spk = bitcoin::Address::p2wpkh(
            &bitcoin::CompressedPublicKey(pk.inner),
            Network::Testnet,
        )
        .script_pubkey();

        let dest = Address::from_str("tb1qw508d6qejxtdg4y5r3zarvary0c5xw7kxpjzsx")
            .unwrap()
            .require_network(Network::Testnet)
            .unwrap();
        let tx = Transaction {
            version: Version::TWO,
            lock_time: LockTime::ZERO,
            input: vec![TxIn {
                previous_output: OutPoint {
                    txid: Txid::from_str(
                        "0000000000000000000000000000000000000000000000000000000000000001",
                    )
                    .unwrap(),
                    vout: 0,
                },
                script_sig: Default::default(),
                sequence: Sequence::ENABLE_RBF_NO_LOCKTIME,
                witness: Witness::new(),
            }],
            output: vec![TxOut {
                value: Amount::from_sat(90_000),
                script_pubkey: dest.script_pubkey(),
            }],
        };
        let mut psbt = Psbt::from_unsigned_tx(tx).unwrap();
        let mut input = Input {
            witness_utxo: Some(TxOut {
                value: Amount::from_sat(100_000),
                script_pubkey: my_spk,
            }),
            ..Default::default()
        };
        input.bip32_derivation.insert(pk.inner, (fingerprint, path));
        psbt.inputs[0] = input;
        psbt
    }

    #[test]
    fn firmware_signature_matches_rust_bitcoin() {
        let wallet = Wallet::from_mnemonic(ABANDON, "", Network::Testnet).unwrap();
        let seed = mnemonic_to_seed(ABANDON, "").unwrap();

        // 参考：用 x86 rust-bitcoin 签名。
        let mut ref_psbt = build_signable_psbt(&wallet);
        let n = btc_wallate_core::btc::sign(&wallet, &mut ref_psbt).unwrap();
        assert_eq!(n, 1);
        let (ref_pk, ref_sig) = ref_psbt.inputs[0].partial_sigs.iter().next().unwrap();

        // 固件：解析同一 PSBT 并签名。
        let unsigned = build_signable_psbt(&wallet).serialize();
        let signed = sign_psbt(&seed, &unsigned).unwrap();

        // 用 rust-bitcoin 解析固件产物，比对 partial_sig 逐字节一致。
        let fw_psbt = Psbt::deserialize(&signed).unwrap();
        let (fw_pk, fw_sig) = fw_psbt.inputs[0].partial_sigs.iter().next().unwrap();
        assert_eq!(fw_pk, ref_pk, "公钥应一致");
        assert_eq!(
            fw_sig.to_vec(),
            ref_sig.to_vec(),
            "固件 BIP-143 签名应与 rust-bitcoin 逐字节一致"
        );
    }

    #[test]
    fn no_ownable_input_errors() {
        // 一个不含本钱包 bip32_derivation 的最简 PSBT。
        let dest = Address::from_str("tb1qw508d6qejxtdg4y5r3zarvary0c5xw7kxpjzsx")
            .unwrap()
            .require_network(Network::Testnet)
            .unwrap();
        let tx = Transaction {
            version: Version::TWO,
            lock_time: LockTime::ZERO,
            input: vec![TxIn {
                previous_output: OutPoint {
                    txid: Txid::from_str(
                        "0000000000000000000000000000000000000000000000000000000000000001",
                    )
                    .unwrap(),
                    vout: 0,
                },
                script_sig: Default::default(),
                sequence: Sequence::ENABLE_RBF_NO_LOCKTIME,
                witness: Witness::new(),
            }],
            output: vec![TxOut {
                value: Amount::from_sat(50_000),
                script_pubkey: dest.script_pubkey(),
            }],
        };
        let seed = mnemonic_to_seed(ABANDON, "").unwrap();
        let psbt = Psbt::from_unsigned_tx(tx).unwrap().serialize();
        assert!(sign_psbt(&seed, &psbt).is_err());
    }
}
