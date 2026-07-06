//! 端到端集成测试：文件通道 → ops → 真实签名。
//!
//! 走完整链路：构造本钱包拥有输入的 PSBT → 写入文件 → `read_signing_input` 读回 →
//! `parse_job` / `load_wallet` / `summarize` / `sign` → 断言签名确实产生。
//! 这覆盖了 CLI `sign --in <file>` 除交互提示外的全部逻辑。

use std::str::FromStr;

use bitcoin::bip32::{ChildNumber, DerivationPath};
use bitcoin::psbt::{Input, Psbt};
use bitcoin::secp256k1::Secp256k1;
use bitcoin::{
    absolute::LockTime, transaction::Version, Address, Amount, Network, OutPoint, Sequence,
    Transaction, TxIn, TxOut, Txid, Witness,
};
use btc_wallate_app::{file_channel, ops};
use btc_wallate_core::airgap::psbt as psbt_ur;
use btc_wallate_core::derive::btc_address;
use btc_wallate_core::seed::Wallet;

const TEST_JUNK: &str = "test test test test test test test test test test test junk";

/// 构造一笔从本钱包 m/84'/1'/0'/0/0 花费、带正确 witness_utxo 与 bip32_derivation 的 PSBT。
fn build_signable_psbt(w: &Wallet) -> Psbt {
    let secp = Secp256k1::new();
    let my_addr = btc_address(w, 0, 0, 0).unwrap();
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

    let dest = Address::from_str("tb1qw508d6qejxtdg4y5r3zarvary0c5xw7kxpjzsx")
        .unwrap()
        .require_network(Network::Testnet)
        .unwrap();

    let unsigned = Transaction {
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

    let mut psbt = Psbt::from_unsigned_tx(unsigned).unwrap();
    let mut input = Input {
        witness_utxo: Some(TxOut {
            value: Amount::from_sat(100_000),
            script_pubkey: my_addr.script_pubkey(),
        }),
        ..Default::default()
    };
    input.bip32_derivation.insert(pk.inner, (fingerprint, path));
    psbt.inputs[0] = input;
    psbt
}

fn tmp_path(name: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!("btc_wallate_it_{name}"))
}

#[test]
fn sign_raw_psbt_file_end_to_end() {
    let w = Wallet::from_mnemonic(TEST_JUNK, "", Network::Testnet).unwrap();

    // 观察钱包侧：把 PSBT 写成原始 .psbt 文件。
    let psbt = build_signable_psbt(&w);
    let path = tmp_path("in.psbt");
    std::fs::write(&path, psbt.serialize()).unwrap();

    // 签名机侧：读文件 → 解析 → 签名。
    let (ur_type, payload) = file_channel::read_signing_input(&path).unwrap();
    assert_eq!(ur_type, psbt_ur::UR_TYPE);
    let job = ops::parse_job(&ur_type, &payload).unwrap();

    let summary = ops::summarize(&w, &job).unwrap();
    assert!(summary.contains("90000 sat"));
    assert!(summary.contains("手续费: 10000 sat"));

    let (out_type, out_payload) = ops::sign(&w, &job).unwrap();
    assert_eq!(out_type, psbt_ur::UR_TYPE);

    // 结果 payload 应为已签名 PSBT（含 partial_sigs）。
    let signed = psbt_ur::from_cbor(&out_payload).unwrap();
    assert!(!signed.inputs[0].partial_sigs.is_empty(), "签名后应有 partial_sigs");

    std::fs::remove_file(&path).ok();
}

#[test]
fn sign_base64_psbt_file() {
    // 模拟 BlueWallet/Nunchuk 导出的 base64 PSBT 文本文件。
    let w = Wallet::from_mnemonic(TEST_JUNK, "", Network::Testnet).unwrap();
    let psbt = build_signable_psbt(&w);
    let in_path = tmp_path("in_b64.txt");
    std::fs::write(&in_path, psbt.to_string()).unwrap(); // Psbt Display = base64

    let (ur_type, payload) = file_channel::read_signing_input(&in_path).unwrap();
    assert_eq!(ur_type, psbt_ur::UR_TYPE);
    let job = ops::parse_job(&ur_type, &payload).unwrap();
    let (out_type, out_payload) = ops::sign(&w, &job).unwrap();

    // write_signed 对 crypto-psbt 应写成 base64，可被再次读入。
    let out_path = tmp_path("out_b64.txt");
    file_channel::write_signed(&out_path, &out_type, &out_payload).unwrap();
    let (_t, rp) = file_channel::read_signing_input(&out_path).unwrap();
    let signed = psbt_ur::from_cbor(&rp).unwrap();
    assert!(!signed.inputs[0].partial_sigs.is_empty());

    std::fs::remove_file(&in_path).ok();
    std::fs::remove_file(&out_path).ok();
}

#[test]
fn read_uppercase_ur_is_handled() {
    // 二维码里的 UR 常为大写（QR 字母数字模式）；读入应大小写不敏感。
    let w = Wallet::from_mnemonic(TEST_JUNK, "", Network::Testnet).unwrap();
    let psbt = build_signable_psbt(&w);
    let cbor = psbt_ur::to_cbor(&psbt).unwrap();
    let ur_upper = btc_wallate_core::airgap::encode_single(psbt_ur::UR_TYPE, &cbor).to_uppercase();
    assert!(ur_upper.starts_with("UR:CRYPTO-PSBT/"));

    let path = tmp_path("upper.ur");
    std::fs::write(&path, &ur_upper).unwrap();
    let (ur_type, payload) = file_channel::read_signing_input(&path).unwrap();
    assert_eq!(ur_type, psbt_ur::UR_TYPE);
    let job = ops::parse_job(&ur_type, &payload).unwrap();
    let (_t, out) = ops::sign(&w, &job).unwrap();
    assert!(!psbt_ur::from_cbor(&out).unwrap().inputs[0]
        .partial_sigs
        .is_empty());
    std::fs::remove_file(&path).ok();
}

#[test]
fn sign_ur_text_file_roundtrip() {
    let w = Wallet::from_mnemonic(TEST_JUNK, "", Network::Testnet).unwrap();
    let psbt = build_signable_psbt(&w);

    // 观察钱包侧：写成 .ur 文本文件（单条 UR）。
    let in_path = tmp_path("in.ur");
    let cbor = psbt_ur::to_cbor(&psbt).unwrap();
    file_channel::write_ur(&in_path, psbt_ur::UR_TYPE, &cbor).unwrap();

    // 签名机侧读回并签名，输出再写成 .ur。
    let (ur_type, payload) = file_channel::read_signing_input(&in_path).unwrap();
    let job = ops::parse_job(&ur_type, &payload).unwrap();
    let (out_type, out_payload) = ops::sign(&w, &job).unwrap();

    let out_path = tmp_path("out.ur");
    file_channel::write_ur(&out_path, &out_type, &out_payload).unwrap();

    // 读回输出文件，确认是已签名 PSBT。
    let (rt, rp) = file_channel::read_signing_input(&out_path).unwrap();
    assert_eq!(rt, psbt_ur::UR_TYPE);
    let signed = psbt_ur::from_cbor(&rp).unwrap();
    assert!(!signed.inputs[0].partial_sigs.is_empty());

    std::fs::remove_file(&in_path).ok();
    std::fs::remove_file(&out_path).ok();
}
