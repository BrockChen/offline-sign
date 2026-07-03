//! BTC 交易解析与签名（PSBT / BIP-174）。
//!
//! 离线签名机对一笔 PSBT 做两件事：
//! 1. [`summarize`]：解析出「给谁、多少、手续费、哪些是找零」，供在签名机自己的屏幕上
//!    强制人工核对——这是空气隙防止被入侵手机偷换收款地址的核心安全属性。
//! 2. [`sign`]：用主私钥对本钱包拥有的输入签名（依赖 PSBT 内的 BIP32 派生信息）。
//!
//! 签名逻辑全部委托给经过审计的 rust-bitcoin，本模块不触碰任何密码学原语。

use bitcoin::psbt::Psbt;
use bitcoin::{Address, Amount};

use crate::{Result, Wallet};

/// 单个输出的人类可读摘要。
#[derive(Debug, Clone)]
pub struct OutputSummary {
    /// 收款地址（无法从脚本还原时为 None，需高危警告）。
    pub address: Option<String>,
    pub amount: Amount,
    /// 该输出是否属于本钱包（即找零 / 自转）。
    pub is_mine: bool,
}

/// 一笔待签 PSBT 的摘要。
#[derive(Debug, Clone)]
pub struct PsbtSummary {
    pub outputs: Vec<OutputSummary>,
    /// 手续费（无法计算时为 None）。
    pub fee: Option<Amount>,
    /// 发往外部地址（非本钱包）的总额。
    pub spent_to_external: Amount,
}

/// 解析 PSBT，产出用于屏幕核对的摘要。
pub fn summarize(wallet: &Wallet, psbt: &Psbt) -> Result<PsbtSummary> {
    let network = wallet.network();
    let fingerprint = wallet.master_xpriv().fingerprint(wallet.secp());

    let mut outputs = Vec::with_capacity(psbt.unsigned_tx.output.len());
    let mut spent_to_external = Amount::ZERO;

    for (i, txout) in psbt.unsigned_tx.output.iter().enumerate() {
        let address = Address::from_script(&txout.script_pubkey, network)
            .ok()
            .map(|a| a.to_string());

        // 输出的 bip32_derivation 里出现本主私钥指纹 ⇒ 属于本钱包（找零/自转）。
        let is_mine = psbt
            .outputs
            .get(i)
            .map(|o| {
                o.bip32_derivation
                    .values()
                    .any(|(fp, _)| *fp == fingerprint)
            })
            .unwrap_or(false);

        if !is_mine {
            spent_to_external += txout.value;
        }

        outputs.push(OutputSummary {
            address,
            amount: txout.value,
            is_mine,
        });
    }

    // rust-bitcoin 会用各输入的 witness_utxo/non_witness_utxo 计算手续费。
    let fee = psbt.fee().ok();

    Ok(PsbtSummary {
        outputs,
        fee,
        spent_to_external,
    })
}

/// 用主私钥对本钱包拥有的输入签名，返回成功签名的输入数量。
///
/// 依赖 PSBT 各输入携带的 `bip32_derivation`（指纹需匹配本主私钥）。签名后 PSBT 内
/// 会填入 partial_sigs / witness；最终 finalize 由观察端或后续 finalize 步骤完成。
pub fn sign(wallet: &Wallet, psbt: &mut Psbt) -> Result<usize> {
    // Xpriv 实现了 GetKey：会按各输入的派生路径自动取私钥签名。
    let signed = match psbt.sign(wallet.master_xpriv(), wallet.secp()) {
        Ok(keys) => keys.len(),
        // 部分输入无法签（非本钱包）是正常情况，返回已成功签名的数量。
        Err((keys, _errors)) => keys.len(),
    };
    Ok(signed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::derive::btc_address;
    use bitcoin::bip32::{ChildNumber, DerivationPath};
    use bitcoin::psbt::Input;
    use bitcoin::{
        transaction::Version, Address, Amount, Network, OutPoint, Sequence, Transaction, TxIn,
        TxOut, Txid, Witness,
    };
    use std::str::FromStr;

    const ABANDON: &str =
        "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";

    // 构造一笔从本钱包 m/84'/1'/0'/0/0 花费的 PSBT，带正确的 witness_utxo 与 bip32_derivation。
    fn build_signable_psbt(w: &Wallet) -> Psbt {
        let secp = w.secp();
        // 我方地址与其脚本（testnet，coin'=1）。
        let my_addr = btc_address(w, 0, 0, 0).unwrap();
        let script = my_addr.script_pubkey();

        // 派生我方输入公钥与路径信息。
        let path = DerivationPath::from(vec![
            ChildNumber::from_hardened_idx(84).unwrap(),
            ChildNumber::from_hardened_idx(1).unwrap(),
            ChildNumber::from_hardened_idx(0).unwrap(),
            ChildNumber::from_normal_idx(0).unwrap(),
            ChildNumber::from_normal_idx(0).unwrap(),
        ]);
        let xpriv = w.master_xpriv().derive_priv(secp, &path).unwrap();
        let pk = bitcoin::PublicKey::new(xpriv.private_key.public_key(secp));
        let fingerprint = w.master_xpriv().fingerprint(secp);

        let prev_amount = Amount::from_sat(100_000);
        let prev_out = OutPoint {
            txid: Txid::from_str(
                "0000000000000000000000000000000000000000000000000000000000000001",
            )
            .unwrap(),
            vout: 0,
        };

        // 一个外部收款输出 + 一个找零输出（找零地址属于本钱包）。
        let dest = Address::from_str("tb1qw508d6qejxtdg4y5r3zarvary0c5xw7kxpjzsx")
            .unwrap()
            .require_network(Network::Testnet)
            .unwrap();
        let change_addr = btc_address(w, 0, 1, 0).unwrap();

        let unsigned = Transaction {
            version: Version::TWO,
            lock_time: bitcoin::absolute::LockTime::ZERO,
            input: vec![TxIn {
                previous_output: prev_out,
                script_sig: Default::default(),
                sequence: Sequence::ENABLE_RBF_NO_LOCKTIME,
                witness: Witness::new(),
            }],
            output: vec![
                TxOut {
                    value: Amount::from_sat(60_000),
                    script_pubkey: dest.script_pubkey(),
                },
                TxOut {
                    value: Amount::from_sat(39_000), // 1000 sat 手续费
                    script_pubkey: change_addr.script_pubkey(),
                },
            ],
        };

        let mut psbt = Psbt::from_unsigned_tx(unsigned).unwrap();

        // 输入：witness_utxo + bip32_derivation。
        let mut input = Input {
            witness_utxo: Some(TxOut {
                value: prev_amount,
                script_pubkey: script,
            }),
            ..Default::default()
        };
        input
            .bip32_derivation
            .insert(pk.inner, (fingerprint, path.clone()));
        psbt.inputs[0] = input;

        // 找零输出也标注 bip32_derivation，便于 summarize 识别为“本钱包”。
        let change_path = DerivationPath::from(vec![
            ChildNumber::from_hardened_idx(84).unwrap(),
            ChildNumber::from_hardened_idx(1).unwrap(),
            ChildNumber::from_hardened_idx(0).unwrap(),
            ChildNumber::from_normal_idx(1).unwrap(),
            ChildNumber::from_normal_idx(0).unwrap(),
        ]);
        let change_xpriv = w.master_xpriv().derive_priv(secp, &change_path).unwrap();
        let change_pk = bitcoin::PublicKey::new(change_xpriv.private_key.public_key(secp));
        psbt.outputs[1]
            .bip32_derivation
            .insert(change_pk.inner, (fingerprint, change_path));

        psbt
    }

    #[test]
    fn summarize_reports_outputs_fee_and_change() {
        let w = Wallet::from_mnemonic(ABANDON, "", Network::Testnet).unwrap();
        let psbt = build_signable_psbt(&w);
        let s = summarize(&w, &psbt).unwrap();

        assert_eq!(s.outputs.len(), 2);
        assert!(!s.outputs[0].is_mine, "外部收款输出不应被识别为本钱包");
        assert!(s.outputs[1].is_mine, "找零输出应被识别为本钱包");
        assert_eq!(s.spent_to_external, Amount::from_sat(60_000));
        assert_eq!(s.fee, Some(Amount::from_sat(1_000)));
    }

    #[test]
    fn sign_populates_signature_for_owned_input() {
        let w = Wallet::from_mnemonic(ABANDON, "", Network::Testnet).unwrap();
        let mut psbt = build_signable_psbt(&w);
        let n = sign(&w, &mut psbt).unwrap();
        assert_eq!(n, 1, "应成功签名 1 个本钱包输入");
        assert!(
            !psbt.inputs[0].partial_sigs.is_empty(),
            "签名后应写入 partial_sigs"
        );
    }
}
